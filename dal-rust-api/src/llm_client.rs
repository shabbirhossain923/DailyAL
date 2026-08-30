use serde_json::json;
use std::time::Duration;

use crate::{config::Config, model::OpenAIResponse};

#[derive(Debug, Clone)]
pub struct LLMClient {
    pub config: Config,
}

impl LLMClient {
    pub async fn talk(
        &self,
        system: &str,
        prompt: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let llm_api_key = self.config.secrets.llm_api_key.as_str();
        let api_url = self.config.secrets.llm_api_url.as_str();
        println!("Calling LLM...");

        let client = reqwest::Client::new();

        let request_body = json!({
            "model": self.config.secrets.llm_model_name,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.6,
            "top_p": 0.7,
            "max_tokens": 4096,
            "stream": false
        });

        // Retry transient LLM/API failures. 503 is returned by the provider
        // when the model is temporarily unavailable or under heavy load.
        const MAX_ATTEMPTS: usize = 4;

        let (status, text) = loop {
            let attempt = 1;
            let mut last_error = None;

            let mut result = None;
            for current_attempt in attempt..=MAX_ATTEMPTS {
                let response = client
                    .post(api_url)
                    .header("Authorization", format!("Bearer {}", llm_api_key))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await;

                match response {
                    Ok(res) => {
                        let status = res.status();
                        let text = res.text().await?;

                        if status.is_success() {
                            result = Some((status, text));
                            break;
                        }

                        let retryable = status.as_u16() == 429 || status.is_server_error();
                        if !retryable || current_attempt == MAX_ATTEMPTS {
                            return Err(format!(
                                "LLM API failed with status {}: {}",
                                status, text
                            )
                            .into());
                        }

                        let delay_secs = 2_u64.pow((current_attempt - 1) as u32);
                        println!(
                            "LLM API returned {} (attempt {}/{}). Retrying in {}s...",
                            status, current_attempt, MAX_ATTEMPTS, delay_secs
                        );
                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    }
                    Err(e) => {
                        last_error = Some(e.to_string());
                        if current_attempt == MAX_ATTEMPTS {
                            return Err(format!(
                                "LLM API request failed after {} attempts: {}",
                                MAX_ATTEMPTS,
                                last_error.unwrap_or_else(|| "unknown error".to_string())
                            )
                            .into());
                        }

                        let delay_secs = 2_u64.pow((current_attempt - 1) as u32);
                        println!(
                            "LLM API request error (attempt {}/{}): {}. Retrying in {}s...",
                            current_attempt, MAX_ATTEMPTS, e, delay_secs
                        );
                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    }
                }
            }

            if let Some(result) = result {
                break result;
            }
        };

        let response: OpenAIResponse = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse JSON: {} | Response Text: '{}'", e, text))?;

        if let Some(choice) = response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err(format!("LLM API returned no choices! Response: {}", text).into())
        }
    }
}
