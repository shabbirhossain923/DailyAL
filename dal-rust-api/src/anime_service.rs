use seahash::SeaHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::hash::Hasher;
use std::sync::{Arc, Mutex};

use crate::model::{
    Anime, AnimeLink, AnimeQuery, Edge, RelatedAnime, RelationType, ReviewResponse,
    ReviewResponseData,
};

use crate::config::Config;
use crate::model_dto::{ContentGraphDTO, ContentNodeDTO};
use async_recursion::async_recursion;
use chrono::{DateTime, Utc};
use futures::{stream, StreamExt};

const REVIEW_SYSTEM: &str = r#"You are an anime/manga review critic. Read all provided user reviews and produce a concise review summary under 500 words.

Rules:
- Pros and cons are optional, but at least one item must be present across both lists.
- Include at most 3 pros and at most 3 cons.
- Include a final verdict.
- Do not hallucinate facts that are not supported by the reviews.
- Do not contradict yourself between pros, cons, and verdict.
- Return ONLY valid JSON matching exactly this structure:
{"data":{"pros":[{"title":"...","description":"..."}],"cons":[{"title":"...","description":"..."}],"verdict":"..."}}
- If there are no meaningful pros or cons, use an empty array for that list.
- Do not wrap the JSON in Markdown code fences."#;

pub struct AnimeService {
    pub config: Config,
    pub mal_api: crate::mal_api::MalAPI,
    pub cache_service: crate::cache_service::CacheService,
    pub ai_service: crate::llm_client::LLMClient,
    pub anime_link_service: crate::anime_link_service::AnimeLinkService,
}

impl AnimeService {
    pub async fn get_related_anime(&self, id: i64) -> Result<ContentGraphDTO, Box<dyn Error>> {
        let mut graph = ContentGraphDTO {
            nodes: HashSet::new(),
            edges: Vec::new(),
        };

        // A root failure is a real graph error. Do not hide it as an empty
        // graph, otherwise the client only sees a misleading "No Content".
        self.get_related_anime_with_graph(id, &mut graph, false, true)
            .await?;

        Ok(graph)
    }

    #[async_recursion]
    pub async fn get_related_anime_with_graph(
        &self,
        id: i64,
        graph: &mut ContentGraphDTO,
        from_cache: bool,
        include_others: bool,
    ) -> Result<(), Box<dyn Error>> {
        // Keep the public method signature for compatibility, but use an
        // iterative breadth-first traversal so deep/large franchises do not
        // grow the async call stack and duplicate targets are fetched once.
        let root = self.get_anime_by_id(id, from_cache).await?;

        graph.nodes.insert(root.into());

        let mut visited: HashSet<i64> = HashSet::new();
        visited.insert(id);
        let mut queue: VecDeque<(i64, bool)> = VecDeque::new();
        queue.push_back((id, include_others));

        while !queue.is_empty() {
            // Keep the MAL request burst deliberately small. A large
            // franchise can otherwise trigger MAL's rate limiter before the
            // retry logic in MalAPI gets a chance to recover.
            let mut batch = Vec::with_capacity(2);
            while batch.len() < 2 {
                let Some(item) = queue.pop_front() else { break };
                batch.push(item);
            }

            let results = stream::iter(batch)
                .map(|(current_id, current_include_others)| async move {
                    match self.get_anime_by_id(current_id, true).await {
                        Ok(anime) => {
                            let edges = self.get_unvisited_edges(
                                current_id,
                                anime.related_anime.clone(),
                                current_include_others,
                            );
                            Some((anime, edges))
                        }
                        Err(error) => {
                            println!(
                                "Graph node fetch failed for anime {}: {}",
                                current_id, error
                            );
                            None
                        }
                    }
                })
                .buffer_unordered(2)
                .collect::<Vec<_>>()
                .await;

            for result in results.into_iter().flatten() {
                let (anime, edges) = result;
                graph.nodes.insert(anime.into());

                for edge in edges {
                    // The API DTO does not implement PartialEq for relation
                    // types, so deduplicate by the relation endpoints.
                    if !graph.edges.iter().any(|existing| {
                        existing.source == edge.source && existing.target == edge.target
                    }) {
                        graph.edges.push(edge.clone().into());
                    }

                    // Mark a target visited before fetching it. This prevents
                    // duplicate requests when several nodes point to it.
                    if visited.insert(edge.target) {
                        queue.push_back((edge.target, false));
                    }
                }
            }
        }

        Ok(())
    }

    async fn get_edges_from_id(&self, id: i64) -> Option<(Anime, Vec<Edge>)> {
        let anime = self.get_anime_by_id(id, true).await.ok()?;
        let vec = anime.related_anime.clone();
        Some((anime, self.get_unvisited_edges(id, vec, false)))
    }

    fn get_unvisited_edges(
        &self,
        id: i64,
        related_anime: Option<Vec<RelatedAnime>>,
        include_others: bool,
    ) -> Vec<Edge> {
        let mut unvisited_edges: Vec<Edge> = Vec::new();
        if let Some(related_anime) = related_anime {
            unvisited_edges.extend(
                related_anime
                    .iter()
                    .filter(|related_anime| {
                        self.valid_relation(&related_anime.relation_type, include_others)
                    })
                    .map(|related_anime| Edge {
                        source: id,
                        target: related_anime.node.id,
                        relation_type: related_anime.relation_type.clone(),
                    }),
            );
        }
        unvisited_edges
    }

    fn filter_by_nodes(&self, edges: Vec<Edge>, nodes: &HashSet<ContentNodeDTO>) -> Vec<Edge> {
        edges
            .iter()
            .filter(|edge| {
                !nodes.contains(&ContentNodeDTO {
                    id: edge.target,
                    ..Default::default()
                })
            })
            .cloned()
            .collect()
    }

    fn valid_relation(&self, relation_type: &RelationType, include_others: bool) -> bool {
        match relation_type {
            RelationType::AlternativeSetting => true,
            RelationType::Sequel => true,
            RelationType::Prequel => true,
            RelationType::AlternativeVersion => true,
            RelationType::SideStory => true,
            RelationType::ParentStory => true,
            RelationType::Summary => true,
            RelationType::FullStory => true,
            RelationType::SpinOff => true,
            RelationType::Character => false,
            RelationType::Other => include_others,
        }
    }

    async fn get_anime_by_id(&self, id: i64, from_cache: bool) -> Result<Anime, Box<dyn Error>> {
        let now = chrono::Utc::now();
        let result = match from_cache {
            true => {
                self.cache_service
                    .get_cache_by_id("anime", id.to_string())
                    .await
            }
            false => None,
        };

        if result.is_none() {
            println!(
                "{}: Cache miss anime: {}",
                now.format("%d/%m/%Y %H:%M:%S"),
                id
            );
            let anime = self.mal_api.get_anime_details(id).await?;
            self.cache_service
                .set_cache_by_id("anime", id.to_string(), &anime, None)
                .await;
            let then = chrono::Utc::now();
            self.log_anime(&anime, "Saved".to_string(), then, now);
            Ok(anime)
        } else {
            let anime = result.unwrap();
            let then = chrono::Utc::now();
            self.log_anime(&anime, "Cache hit".to_string(), then, now);
            Ok(anime)
        }
    }

    fn log_anime(
        &self,
        anime: &Anime,
        hit_or_miss: String,
        then: DateTime<Utc>,
        now: DateTime<Utc>,
    ) {
        println!(
            "{}: {} anime: {} and {} in {}ms",
            then.format("%d/%m/%Y %H:%M:%S"),
            hit_or_miss,
            anime.id,
            anime.title,
            then.timestamp_millis() - now.timestamp_millis()
        );
    }

    pub fn hash_str(&self, s: &str) -> String {
        let mut hasher = SeaHasher::new();
        hasher.write(s.as_bytes());
        let finish = hasher.finish();
        format!("{:x}", finish)
    }

    pub async fn summarize_review(
        &self,
        reviews: &str,
    ) -> Result<ReviewResponse, Box<dyn Error + Send + Sync>> {
        println!("Summarizing review {}", reviews.len());

        let hash_str = self.hash_str(reviews);

        println!("Using hash_key: {}", hash_str);

        let cached_review: Option<ReviewResponse> = self
            .cache_service
            .get_cache_by_id("reviews", hash_str.clone())
            .await;

        if cached_review.is_some() {
            return Ok(cached_review.unwrap());
        }

        println!("Cache miss for {}", hash_str);
        let json_str = self.ai_service.talk(REVIEW_SYSTEM, reviews).await?;
        let review_response_data: ReviewResponseData = serde_json::from_str(&json_str)?;
        let review_response = review_response_data.data.clone();

        self.cache_service
            .set_cache_by_id("reviews", hash_str, &review_response, Some(3600 * 24 * 30))
            .await;
        Ok(review_response)
    }

    pub async fn get_anime(&self, query: AnimeQuery) -> Vec<HashMap<String, String>> {
        let link: Vec<AnimeLink>;
        if query.query.is_some() {
            link = self.get_anime_by_query(&query).await;
        } else if query.mal_id.is_some() {
            link = self.get_anime_by_mal_id(&query).await;
        } else {
            link = self.get_all_anime().await;
        }
        create_map_using_fields(link, &query.fields)
    }

    async fn get_all_anime(&self) -> Vec<AnimeLink> {
        self.anime_link_service.get_all_anime().await
    }

    async fn get_anime_by_mal_id(&self, query: &AnimeQuery) -> Vec<AnimeLink> {
        let mal_id = &query.mal_id.clone().unwrap().clone();
        let anime_link: AnimeLink = self.anime_link_service.get_link_by_id(mal_id).await;
        Vec::from([anime_link])
    }

    async fn get_anime_by_query(&self, query: &AnimeQuery) -> Vec<AnimeLink> {
        self.anime_link_service.search(query).await
    }
}

fn create_map_using_fields(
    link: Vec<AnimeLink>,
    fields: &Vec<String>,
) -> Vec<HashMap<String, String>> {
    let mut map: Vec<HashMap<String, String>> = Vec::new();
    for anime in &link {
        let mut hash_map: HashMap<String, String> = HashMap::new();
        for field in fields.iter() {
            match field.as_str() {
                "title" => {
                    if anime.title.is_some() {
                        hash_map.insert("title".to_string(), anime.title.clone().unwrap_or_default());
                    }
                }
                "malId" => {
                    if anime.mal_id.is_some() {
                        hash_map.insert("malId".to_string(), anime.mal_id.clone().unwrap_or_default());
                    }
                }
                "anilistId" => {
                    if anime.anilist_id.is_some() {
                        hash_map.insert("anilistId".to_string(), anime.anilist_id.clone().unwrap_or_default());
                    }
                }
                "kitsuId" => {
                    if anime.kitsu_id.is_some() {
                        hash_map.insert("kitsuId".to_string(), anime.kitsu_id.clone().unwrap_or_default());
                    }
                }
                "animePlanet" => {
                    if anime.anime_planet.is_some() {
                        hash_map.insert("animePlanet".to_string(), anime.anime_planet.clone().unwrap_or_default());
                    }
                }
                "picture" => {
                    if anime.picture.is_some() {
                        hash_map.insert("picture".to_string(), anime.picture.clone().unwrap_or_default());
                    }
                }
                "synonyms" => {
                    if anime.synonyms.is_some() {
                        hash_map.insert(
                            "synonyms".to_string(),
                            anime.synonyms.clone().unwrap_or_default().join(",").to_string(),
                        );
                    }
                }
                "year" => {
                    if anime.year.is_some() {
                        hash_map.insert("year".to_string(), anime.year.clone().unwrap_or_default());
                    }
                }
                "mean" => {
                    hash_map.insert("mean".to_string(), anime.mean.to_string());
                }
                _ => {}
            }
        }
        map.push(hash_map);
    }
    map
}