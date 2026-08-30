use crate::config::Config;
use crate::model::Anime;
use crate::reqwest;
use std::error::Error;
use std::io;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MalAPI {
    pub config: Config,
    pub client: reqwest::Client,
}

impl MalAPI {
    pub async fn get_anime_details(
        &self,
        id: i64,
    ) -> Result<Anime, Box<dyn Error>> {
        let fields = "?fields=alternative_titles,mean,media_type,status,start_season,related_anime";
        let url = format!("{}/anime/{}{}", self.config.base_url, id, fields);

        // MAL can rate-limit bursts of graph requests. Retry transient failures
        // with a small exponential backoff instead of silently turning them
        // into an empty graph.
        for attempt in 0..3u32 {
            let response = self
                .client
                .get(&url)
                .header("X-MAL-Client-ID", self.config.secrets.mal_client_id.clone())
                .send()
                .await?;

            let status = response.status();
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let body = response.text().await?;

            if status.is_success() {
                return serde_json::from_str(&body).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("MAL returned invalid JSON for anime {}: {}", id, error),
                    )
                    .into()
                });
            }

            let transient = status.as_u16() == 429 || status.is_server_error();
            if transient && attempt < 2 {
                let delay_seconds = retry_after.unwrap_or(1u64 << attempt);
                println!(
                    "MAL transient error for anime {}: HTTP {}. Retrying in {}s (attempt {}/3)",
                    id,
                    status.as_u16(),
                    delay_seconds,
                    attempt + 1
                );
                tokio::time::sleep(Duration::from_secs(delay_seconds.min(8))).await;
                continue;
            }

            let body_preview: String = body.chars().take(300).collect();
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "MAL API error for anime {}: HTTP {} - {}",
                    id,
                    status.as_u16(),
                    body_preview
                ),
            )
            .into());
        }

        unreachable!("MAL retry loop always returns");
    }
}
