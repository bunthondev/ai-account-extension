pub mod claude;
pub mod gemini;
pub mod openai;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use crate::models::{AccountProfile, ChatMessage, ProviderKind, StreamEvent};
use self::claude::ClaudeClient;
use self::gemini::GeminiClient;
use self::openai::OpenAiClient;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn stream_completion(
        &self,
        history: &[ChatMessage],
        sender: UnboundedSender<StreamEvent>,
    ) -> Result<()>;
}

pub fn create_client(account: &AccountProfile) -> Box<dyn LlmClient> {
    match account.provider {
        ProviderKind::GoogleGemini => Box::new(GeminiClient::new(account)),
        ProviderKind::AnthropicClaude => Box::new(ClaudeClient::new(account)),
        ProviderKind::OpenAiCodex => Box::new(OpenAiClient::new(account)),
    }
}
