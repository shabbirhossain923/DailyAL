use std::collections::{HashSet, VecDeque};
use std::error::Error;

use futures::{stream, StreamExt};

use crate::anime_service::AnimeService;
use crate::model::{Anime, Edge, RelatedAnime, RelationType};
use crate::model_dto::{ContentGraphDTO, ContentNodeDTO};

/// Build a related-anime graph without recursively re-fetching the root.
///
/// The important detail here is that `buffered` preserves the input order,
/// unlike `buffer_unordered`. MAL returns `related_anime` in a meaningful
/// order, so preserving that order keeps the graph layout stable between
/// requests while still allowing a small amount of concurrency.
pub async fn get_related_anime(
    service: &AnimeService,
    id: i64,
) -> Result<ContentGraphDTO, Box<dyn Error>> {
    let root = get_anime_by_id(service, id).await?;

    let mut graph = ContentGraphDTO {
        nodes: HashSet::new(),
        edges: Vec::new(),
    };
    graph.nodes.insert(root.clone().into());

    let mut visited = HashSet::new();
    visited.insert(id);

    let root_edges = get_edges(id, root.related_anime.clone(), true);
    for edge in root_edges {
        if visited.insert(edge.target) {
            graph.edges.push(edge.clone().into());
        }
    }

    let mut queue: VecDeque<i64> = graph
        .edges
        .iter()
        .map(|edge| edge.target)
        .collect();

    while !queue.is_empty() {
        // Keep the burst deliberately small. The cache makes repeated graph
        // traversals cheap, while a small MAL burst reduces rate-limit risk.
        let mut batch = Vec::with_capacity(2);
        while batch.len() < 2 {
            let Some(current_id) = queue.pop_front() else { break };
            batch.push(current_id);
        }

        // `buffered`, rather than `buffer_unordered`, is intentional: it
        // preserves the order of the batch and therefore the order of edges
        // entering GraphView/Sugiyama.
        let results = stream::iter(batch)
            .map(|current_id| async move {
                match get_anime_by_id(service, current_id).await {
                    Ok(anime) => Some((current_id, anime)),
                    Err(error) => {
                        println!(
                            "Graph node fetch failed for anime {}: {}",
                            current_id, error
                        );
                        None
                    }
                }
            })
            .buffered(2)
            .collect::<Vec<_>>()
            .await;

        for result in results.into_iter().flatten() {
            let (current_id, anime) = result;
            graph.nodes.insert(anime.clone().into());

            // Match the original graph semantics: only add an edge when its
            // target is a new node. This prevents cycles/cross-links from
            // making the franchise look unordered or overly tangled.
            for edge in get_edges(current_id, anime.related_anime.clone(), false) {
                if visited.insert(edge.target) {
                    graph.edges.push(edge.clone().into());
                    queue.push_back(edge.target);
                }
            }
        }
    }

    Ok(graph)
}

async fn get_anime_by_id(service: &AnimeService, id: i64) -> Result<Anime, Box<dyn Error>> {
    if let Some(anime) = service
        .cache_service
        .get_cache_by_id::<Anime>("anime", id.to_string())
        .await
    {
        return Ok(anime);
    }

    let anime = service.mal_api.get_anime_details(id).await?;
    service
        .cache_service
        .set_cache_by_id("anime", id.to_string(), &anime, None)
        .await;
    Ok(anime)
}

fn get_edges(
    id: i64,
    related_anime: Option<Vec<RelatedAnime>>,
    include_others: bool,
) -> Vec<Edge> {
    related_anime
        .unwrap_or_default()
        .into_iter()
        .filter(|related| valid_relation(&related.relation_type, include_others))
        .map(|related| Edge {
            source: id,
            target: related.node.id,
            relation_type: related.relation_type,
        })
        .collect()
}

fn valid_relation(relation_type: &RelationType, include_others: bool) -> bool {
    match relation_type {
        RelationType::AlternativeSetting
        | RelationType::Sequel
        | RelationType::Prequel
        | RelationType::AlternativeVersion
        | RelationType::SideStory
        | RelationType::ParentStory
        | RelationType::Summary
        | RelationType::FullStory
        | RelationType::SpinOff => true,
        RelationType::Character => false,
        RelationType::Other => include_others,
    }
}

// Keep the conversion visible to the compiler when this module is used with
// a DTO-only graph response.
fn _node(_anime: Anime) -> ContentNodeDTO {
    _anime.into()
}
