use seahash::SeaHasher;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::hash::Hasher;

use crate::model::{
    Anime, AnimeLink, AnimeQuery, Edge, RelatedAnime, RelationType, ReviewResponse,
    ReviewResponseData,
};

use crate::config::Config;
use crate::model_dto::ContentGraphDTO;
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

        self.get_related_anime_with_graph(id, &mut graph, false, true)
            .await?;

        Ok(graph)
    }

    pub async fn get_related_anime_with_graph(
        &self,
        id: i64,
        graph: &mut ContentGraphDTO,
        from_cache: bool,
        include_others: bool,
    ) -> Result<(), Box<dyn Error>> {
        // Build the graph as a franchise graph, not as a watch-order graph.
        // MAL relations can contain weak/crossover links, so every traversed
        // relation must still belong to the same franchise as the root.
        let root = self.get_anime_by_id(id, from_cache).await?;
        let franchise_tokens = Self::franchise_tokens(&root);

        let mut visited: HashSet<i64> = HashSet::new();
        visited.insert(id);

        // Each frontier already contains fetched Anime values. This avoids
        // fetching the root twice and lets the next frontier be fetched in
        // parallel with a small, rate-limit-friendly concurrency cap.
        let mut frontier = vec![(root, include_others)];

        while !frontier.is_empty() {
            let mut next_ids = Vec::new();

            for (anime, allow_other) in frontier.drain(..) {
                let current_id = anime.id;
                graph.nodes.insert(anime.clone().into());

                let edges = self.get_unvisited_edges(
                    current_id,
                    anime.related_anime.clone(),
                    allow_other,
                    &franchise_tokens,
                );

                for edge in edges {
                    // Preserve different relation types between the same pair.
                    if !graph.edges.iter().any(|existing| {
                        existing.source == edge.source
                            && existing.target == edge.target
                            && format!("{:?}", existing.relation_type)
                                == format!("{:?}", edge.relation_type)
                    }) {
                        graph.edges.push(edge.clone().into());
                    }

                    if visited.insert(edge.target) {
                        next_ids.push(edge.target);
                    }
                }
            }

            let fetched = stream::iter(next_ids)
                .map(|target| async move {
                    match self.get_anime_by_id(target, true).await {
                        Ok(anime) => Some((anime, false)),
                        Err(error) => {
                            println!(
                                "Graph node fetch failed for anime {}: {}",
                                target, error
                            );
                            None
                        }
                    }
                })
                .buffer_unordered(5)
                .collect::<Vec<_>>()
                .await;

            frontier = fetched.into_iter().flatten().collect();
        }

        Ok(())
    }

    fn get_unvisited_edges(
        &self,
        id: i64,
        related_anime: Option<Vec<RelatedAnime>>,
        include_others: bool,
        franchise_tokens: &HashSet<String>,
    ) -> Vec<Edge> {
        related_anime
            .unwrap_or_default()
            .into_iter()
            .filter(|related| {
                self.valid_relation(&related.relation_type, include_others)
                    && Self::same_franchise(&related.node.title, franchise_tokens)
            })
            .map(|related| Edge {
                source: id,
                target: related.node.id,
                relation_type: related.relation_type,
            })
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

    fn franchise_tokens(anime: &Anime) -> HashSet<String> {
        let mut titles = vec![anime.title.clone()];
        if let Some(alternative) = &anime.alternative_titles {
            if let Some(en) = &alternative.en {
                titles.push(en.clone());
            }
            if let Some(ja) = &alternative.ja {
                titles.push(ja.clone());
            }
        }

        let generic = [
            "season", "part", "movie", "special", "ova", "ona", "oad", "tv", "the",
            "final", "chapter", "episode", "edition", "version", "story", "series",
        ];

        titles
            .iter()
            .flat_map(|title| {
                title
                    .to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|token| token.len() >= 4 && !generic.contains(token))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn same_franchise(title: &str, franchise_tokens: &HashSet<String>) -> bool {
        if franchise_tokens.is_empty() {
            return true;
        }

        title
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .any(|token| token.len() >= 4 && franchise_tokens.contains(token))
    }

    async fn get_anime_by_id(&self, id: i64, from_cache: bool) -> Result<Anime, Box<dyn Error>> {
        let now = chrono::Utc::now();
        let result = match from_cache {
            true => self
                .cache_service
                .get_cache_by_id("anime", id.to_string())
                .await,
            false => None,
        };

        if let Some(anime) = result {
            let then = chrono::Utc::now();
            self.log_anime(&anime, "Cache hit".to_string(), then, now);
            return Ok(anime);
        }

        println!(
            "{}: Cache miss anime: {}",
            now.format("%d/%m/%Y %H:%M:%S"),
            id
        );

        let anime = self.mal_api.get_anime_details(id).await?;

        // Do not make the graph request wait for the persistent cache write.
        // set_cache_by_id updates the in-memory cache before the DynamoDB write,
        // so later graph nodes in this same request can still hit the cache.
        let cache_service = self.cache_service.clone();
        let cache_anime = anime.clone();
        let cache_id = id.to_string();
        tokio::spawn(async move {
            let _ = cache_service
                .set_cache_by_id("anime", cache_id, &cache_anime, None)
                .await;
        });

        let then = chrono::Utc::now();
        self.log_anime(&anime, "Fetched".to_string(), then, now);
        Ok(anime)
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
