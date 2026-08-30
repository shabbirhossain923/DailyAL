use crate::config::Config;
use crate::model::Anime;
use crate::reqwest;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone)]
pub struct MalAPI {
    pub config: Config,
    pub client: reqwest::Client,
}

impl MalAPI {
    pub async fn get_anime_details(
        &self,
        id: i64,
    ) -> Result<Anime, Box<dyn std::error::Error>> {
        let fields = "?fields=alternative_titles,mean,media_type,status,start_season,related_anime";
        let url = format!("{}/anime/{}{}", self.config.base_url, id, fields);
        let client = &self.client;
        let client_id = self.config.secrets.mal_client_id.clone();

        let mut last_error = String::from("unknown MyAnimeList API error");

        for attempt in 0..3 {
            let response = client
                .get(&url)
                .header("X-MAL-Client-ID", client_id.clone())
                .send()
                .await;

            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    last_error = format!("request error: {}", error);
                    if attempt < 2 {
                        sleep(Duration::from_secs(1 << attempt)).await;
                        continue;
                    }
                    break;
                }
            };

            let status = response.status();
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let body = response.text().await?;

            if status.is_success() {
                let anime: Anime = serde_json::from_str(&body)?;
                return Ok(anime);
            }

            last_error = format!(
                "HTTP {} from MyAnimeList for anime {}: {}",
                status,
                id,
                body.chars().take(300).collect::<String>()
            );

            // MAL may throttle bursty graph traversal with 403/429. Retry those,
            // as well as temporary 5xx responses, using exponential backoff.
            if (status == reqwest::StatusCode::FORBIDDEN
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error())
                && attempt < 2
            {
                let delay = retry_after.unwrap_or(1 << attempt);
                sleep(Duration::from_secs(delay.min(10))).await;
                continue;
            }

            break;
        }

        Err(last_error.into())
    }
}
