use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use crate::models::{AccountProfile, ChatMessage, Role, StreamEvent};
use super::LlmClient;

pub struct OpenAiClient {
    api_key: String,
    model: String,
    base_url: Option<String>,
    http_client: Client,
}

impl OpenAiClient {
    pub fn new(account: &AccountProfile) -> Self {
        Self {
            api_key: account.api_key.clone(),
            model: account.model.clone(),
            base_url: account.base_url.clone(),
            http_client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn stream_completion(
        &self,
        history: &[ChatMessage],
        sender: UnboundedSender<StreamEvent>,
    ) -> Result<()> {
        let endpoint = self.base_url.as_deref().unwrap_or("https://api.openai.com");
        let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));

        let mut messages = Vec::new();
        for msg in history {
            let role_str = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };
            messages.push(json!({
                "role": role_str,
                "content": msg.content
            }));
        }

        let body = json!({
            "model": self.model,
            "stream": true,
            "messages": messages
        });

        let resp = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            let _ = sender.send(StreamEvent::Error(format!(
                "OpenAI API error ({}): {}",
                status, err_text
            )));
            return Err(anyhow!("OpenAI API error ({}): {}", status, err_text));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error reading OpenAI response stream")?;
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
                    if data_str == "[DONE]" {
                        let _ = sender.send(StreamEvent::Finished);
                        return Ok(());
                    }
                    if data_str.is_empty() {
                        continue;
                    }

                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) {
                        if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
                            if let Some(first) = choices.first() {
                                if let Some(delta) = first.get("delta") {
                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                        let _ = sender.send(StreamEvent::Chunk(content.to_string()));
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
