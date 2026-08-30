use serde_json::{json, Value};
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
        let primary_model = self.config.secrets.llm_model_name.as_str();

        // Optional fallback. This is deliberately read at runtime so existing
        // Render deployments do not need a new environment variable.
        // Gemini 3.1 Flash-Lite is a stable, high-throughput fallback model.
        let fallback_model = std::env::var("LLM_FALLBACK_MODEL_NAME")
            .unwrap_or_else(|_| "gemini-3.1-flash-lite".to_string());

        println!("Calling LLM with model: {}", primary_model);

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .build()?;

        let models: Vec<&str> = if fallback_model == primary_model {
            vec![primary_model]
        } else {
            vec![primary_model, fallback_model.as_str()]
        };

        const ATTEMPTS_PER_MODEL: usize = 3;

        let mut last_error = String::from("LLM request failed without a response");

        for model in models {
            let request_body = json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0.6,
                "top_p": 0.7,
                "max_tokens": 4096,
                "stream": false,
                "response_format": {"type": "json_object"}
            });

            for attempt in 1..=ATTEMPTS_PER_MODEL {
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
                        let retry_after = res
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok());
                        let text = res.text().await?;

                        if status.is_success() {
                            match extract_content(&text) {
                                Some(content) if !content.trim().is_empty() => {
                                    println!(
                                        "LLM succeeded with model {} on attempt {}/{}",
                                        model, attempt, ATTEMPTS_PER_MODEL
                                    );
                                    return Ok(content);
                                }
                                _ => {
                                    last_error = format!(
                                        "LLM returned a successful response with no usable assistant content: {}",
                                        truncate(&text, 1000)
                                    );
                                    println!(
                                        "LLM returned no usable content with model {} (attempt {}/{}).",
                                        model, attempt, ATTEMPTS_PER_MODEL
                                    );
                                }
                            }
                        } else {
                            last_error = format!(
                                "LLM API failed with status {}: {}",
                                status,
                                truncate(&text, 1500)
                            );

                            let retryable = matches!(
                                status.as_u16(),
                                408 | 409 | 425 | 429
                            ) || status.is_server_error();

                            println!(
                                "LLM API returned {} with model {} (attempt {}/{}).{}",
                                status,
                                model,
                                attempt,
                                ATTEMPTS_PER_MODEL,
                                if retryable { " Retrying if possible..." } else { "" }
                            );

                            if !retryable {
                                break;
                            }
                        }

                        if attempt < ATTEMPTS_PER_MODEL {
                            let delay_secs = retry_after
                                .unwrap_or_else(|| 2_u64.pow((attempt - 1) as u32));
                            tokio::time::sleep(Duration::from_secs(delay_secs.min(30))).await;
                        }
                    }
                    Err(e) => {
                        last_error = format!(
                            "LLM API request failed with model {}: {}",
                            model, e
                        );

                        if attempt < ATTEMPTS_PER_MODEL {
                            let delay_secs = 2_u64.pow((attempt - 1) as u32);
                            println!(
                                "LLM request error with model {} (attempt {}/{}): {}. Retrying in {}s...",
                                model, attempt, ATTEMPTS_PER_MODEL, e, delay_secs
                            );
                            tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                        }
                    }
                }
            }

            if model != primary_model {
                println!("Fallback model {} also failed.", model);
            } else {
                println!(
                    "Primary model {} exhausted. Trying fallback model {}.",
                    primary_model, fallback_model
                );
            }
        }

        Err(last_error.into())
    }
}

fn extract_content(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;

    // OpenAI-compatible response: choices[0].message.content
    if let Some(content) = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(content_to_string)
    {
        return Some(content);
    }

    // Gemini-style response: candidates[0].content.parts[*].text
    if let Some(parts) = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(Value::as_array)
    {
        let content = parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");

        if !content.trim().is_empty() {
            return Some(content);
        }
    }

    None
}

fn content_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(clean_content(s)),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| {
                    part.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| part.as_str())
                })
                .collect::<Vec<_>>()
                .join("");
            (!text.trim().is_empty()).then(|| clean_content(&text))
        }
        _ => None,
    }
}

fn clean_content(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.starts_with("```json") && trimmed.ends_with("```") {
        return trimmed[7..trimmed.len() - 3].trim().to_string();
    }
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        return trimmed[3..trimmed.len() - 3].trim().to_string();
    }
    trimmed.to_string()
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
