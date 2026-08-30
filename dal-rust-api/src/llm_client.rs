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
            "stream": false,
            "tool_choice": "none",
            "response_format": {"type": "json_object"}
        });

        // Retry transient provider failures such as 429/5xx and successful
        // responses that contain no usable assistant text.
        const MAX_ATTEMPTS: usize = 4;

        let text = {
            let mut final_text = None;

            for attempt in 1..=MAX_ATTEMPTS {
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
                            let has_content = serde_json::from_str::<OpenAIResponse>(&text)
                                .ok()
                                .and_then(|response| response.choices.first().cloned())
                                .map(|choice| !choice.message.content.trim().is_empty())
                                .unwrap_or(false);

                            if has_content || attempt == MAX_ATTEMPTS {
                                final_text = Some(text);
                                break;
                            }

                            println!(
                                "LLM API returned an empty assistant response (attempt {}/{}). Retrying...",
                                attempt, MAX_ATTEMPTS
                            );
                        } else {
                            let retryable = status.as_u16() == 429 || status.is_server_error();
                            if !retryable || attempt == MAX_ATTEMPTS {
                                println!("LLM API failed with status {}: {}", status, text);
                                return Err(
                                    format!("LLM API failed with status {}: {}", status, text).into()
                                );
                            }

                            println!(
                                "LLM API returned {} (attempt {}/{}). Retrying...",
                                status, attempt, MAX_ATTEMPTS
                            );
                        }

                        if attempt < MAX_ATTEMPTS {
                            let delay_secs = 2_u64.pow((attempt - 1) as u32);
                            tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                        }
                    }
                    Err(e) => {
                        if attempt == MAX_ATTEMPTS {
                            return Err(format!(
                                "LLM API request failed after {} attempts: {}",
                                MAX_ATTEMPTS, e
                            )
                            .into());
                        }

                        let delay_secs = 2_u64.pow((attempt - 1) as u32);
                        println!(
                            "LLM API request error (attempt {}/{}): {}. Retrying in {}s...",
                            attempt, MAX_ATTEMPTS, e, delay_secs
                        );
                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    }
                }
            }

            match final_text {
                Some(text) => text,
                None => return Err(std::io::Error::other("LLM API request failed without a response").into()),
            }
        };

        let response: OpenAIResponse = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse JSON: {} | Response Text: '{}'", e, text))?;

        if let Some(choice) = response.choices.first() {
            let content = choice.message.content.trim();
            if content.is_empty() {
                return Err("LLM API returned an empty assistant response".into());
            }
            Ok(content.to_string())
        } else {
            Err(format!("LLM API returned no choices! Response: {}", text).into())
        }
    }
}
