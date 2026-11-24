use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use crate::models::{AccountProfile, ChatMessage, Role, StreamEvent};
use super::LlmClient;

pub struct ClaudeClient {
    api_key: String,
    model: String,
    base_url: Option<String>,
    http_client: Client,
}

impl ClaudeClient {
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
impl LlmClient for ClaudeClient {
    async fn stream_completion(
        &self,
        history: &[ChatMessage],
        sender: UnboundedSender<StreamEvent>,
    ) -> Result<()> {
        let endpoint = self.base_url.as_deref().unwrap_or("https://api.anthropic.com");
        let url = format!("{}/v1/messages", endpoint.trim_end_matches('/'));

        let mut system_prompts = Vec::new();
        let mut messages = Vec::new();

        for msg in history {
            match msg.role {
                Role::System => {
                    system_prompts.push(msg.content.as_str());
                }
                Role::User => {
                    messages.push(json!({
                        "role": "user",
                        "content": msg.content
                    }));
                }
                Role::Assistant => {
                    messages.push(json!({
                        "role": "assistant",
                        "content": msg.content
                    }));
                }
            }
        }

        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "stream": true,
            "messages": messages
        });

        if !system_prompts.is_empty() {
            body["system"] = json!(system_prompts.join("\n\n"));
        }

        let resp = self
            .http_client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Anthropic Claude API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            let _ = sender.send(StreamEvent::Error(format!(
                "Claude API error ({}): {}",
                status, err_text
            )));
            return Err(anyhow!("Claude API error ({}): {}", status, err_text));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error reading Claude response stream")?;
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
                        let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or_default();
                        if event_type == "content_block_delta" {
                            if let Some(delta) = val.get("delta") {
                                if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                    let _ = sender.send(StreamEvent::Chunk(text.to_string()));
                                }
                            }
                        } else if event_type == "message_stop" {
                            let _ = sender.send(StreamEvent::Finished);
                            return Ok(());
                        } else if event_type == "error" {
                            let msg = val.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("Unknown error");
                            let _ = sender.send(StreamEvent::Error(msg.to_string()));
                            return Err(anyhow!("Claude error: {}", msg));
                        }
                    }
                }
            }
        }

        let _ = sender.send(StreamEvent::Finished);
        Ok(())
    }
}
