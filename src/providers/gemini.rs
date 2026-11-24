use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use crate::models::{AccountProfile, ChatMessage, Role, StreamEvent};
use super::LlmClient;

pub struct GeminiClient {
    api_key: String,
    model: String,
    base_url: Option<String>,
    is_oauth: bool,
    http_client: Client,
}

impl GeminiClient {
    pub fn new(account: &AccountProfile) -> Self {
        let is_oauth = account.is_oauth() || account.api_key.starts_with("ya29.");
        Self {
            api_key: account.api_key.clone(),
            model: account.model.clone(),
            base_url: account.base_url.clone(),
            is_oauth,
            http_client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn stream_completion(
        &self,
        history: &[ChatMessage],
        sender: UnboundedSender<StreamEvent>,
    ) -> Result<()> {
        let endpoint = self.base_url.as_deref().unwrap_or(
            "https://generativelanguage.googleapis.com"
        );
        let url = if self.is_oauth {
            format!(
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
                endpoint.trim_end_matches('/'),
                self.model
            )
        } else {
            format!(
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
                endpoint.trim_end_matches('/'),
                self.model,
                self.api_key
            )
        };

        let mut contents = Vec::new();
        let mut system_instruction_parts = Vec::new();

        for msg in history {
            match msg.role {
                Role::System => {
                    system_instruction_parts.push(json!({ "text": msg.content }));
                }
                Role::User => {
                    contents.push(json!({
                        "role": "user",
                        "parts": [{ "text": msg.content }]
                    }));
                }
                Role::Assistant => {
                    contents.push(json!({
                        "role": "model",
                        "parts": [{ "text": msg.content }]
                    }));
                }
            }
        }

        let mut body = json!({
            "contents": contents
        });

        if !system_instruction_parts.is_empty() {
            body["systemInstruction"] = json!({
                "parts": system_instruction_parts
            });
        }

        let mut req = self.http_client.post(&url);
        if self.is_oauth {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Google Gemini API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            
            let user_friendly_msg = if err_text.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT") {
                "Google Gemini rejected this OAuth token (insufficient scope). For personal Google accounts, please use a free API Key from Google AI Studio (aistudio.google.com/app/apikey) via [Ctrl+A] or [Ctrl+W].".to_string()
            } else {
                format!("Gemini API error ({}): {}", status, err_text)
            };

            let _ = sender.send(StreamEvent::Error(user_friendly_msg.clone()));
            return Err(anyhow!("{}", user_friendly_msg));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error reading Gemini response stream")?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer.drain(..=newline_pos);

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data_str) = line.strip_prefix("data:") {
                    let data_str = data_str.trim();
                    if data_str.is_empty() {
                        continue;
                    }

                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) {
                        if let Some(candidates) = val.get("candidates").and_then(|c| c.as_array()) {
                            if let Some(first) = candidates.first() {
                                if let Some(parts) = first
                                    .get("content")
                                    .and_then(|c| c.get("parts"))
                                    .and_then(|p| p.as_array())
                                {
                                    for part in parts {
                                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                            let _ = sender.send(StreamEvent::Chunk(text.to_string()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let _ = sender.send(StreamEvent::Finished);
        Ok(())
    }
}
