use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "gemini")]
    GoogleGemini,
    #[serde(rename = "claude")]
    AnthropicClaude,
    #[serde(rename = "openai")]
    OpenAiCodex,
}

impl ProviderKind {
    pub fn all() -> &'static [ProviderKind] {
        &[
            ProviderKind::GoogleGemini,
            ProviderKind::AnthropicClaude,
            ProviderKind::OpenAiCodex,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ProviderKind::GoogleGemini => "Google Gemini",
            ProviderKind::AnthropicClaude => "Anthropic Claude",
            ProviderKind::OpenAiCodex => "OpenAI / Codex",
        }
    }

    pub fn short_badge(&self) -> &'static str {
        match self {
            ProviderKind::GoogleGemini => "GEMINI",
            ProviderKind::AnthropicClaude => "CLAUDE",
            ProviderKind::OpenAiCodex => "OPENAI",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderKind::GoogleGemini => "gemini-2.5-flash",
            ProviderKind::AnthropicClaude => "claude-3-5-sonnet-20241022",
            ProviderKind::OpenAiCodex => "gpt-4o",
        }
    }

    #[allow(dead_code)]
    pub fn popular_models(&self) -> &'static [&'static str] {
        match self {
            ProviderKind::GoogleGemini => &[
                "gemini-2.5-flash",
                "gemini-1.5-pro",
                "gemini-1.5-flash",
            ],
            ProviderKind::AnthropicClaude => &[
                "claude-3-5-sonnet-20241022",
                "claude-3-5-haiku-20241022",
                "claude-3-opus-20240229",
            ],
            ProviderKind::OpenAiCodex => &[
                "gpt-4o",
                "gpt-4o-mini",
                "o3-mini",
                "o1-mini",
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthType {
    #[serde(rename = "api_key")]
    ApiKey,
    #[serde(rename = "oauth")]
    OAuth {
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        client_id: Option<String>,
        #[serde(default)]
        client_secret: Option<String>,
        #[serde(default)]
        expires_at: Option<DateTime<Utc>>,
        #[serde(default)]
        email: Option<String>,
    },
}

fn default_auth_type() -> AuthType {
    AuthType::ApiKey
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountProfile {
    pub id: String,
    pub name: String,
    pub provider: ProviderKind,
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
    #[serde(default = "default_auth_type")]
    pub auth_type: AuthType,
    pub created_at: DateTime<Utc>,
}

impl AccountProfile {
    pub fn new(
        name: impl Into<String>,
        provider: ProviderKind,
        api_key: impl Into<String>,
        model: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        let default_m = provider.default_model().to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            provider,
            api_key: api_key.into(),
            model: model.unwrap_or(default_m),
            base_url,
            auth_type: AuthType::ApiKey,
            created_at: Utc::now(),
        }
    }

    pub fn new_oauth(
        name: impl Into<String>,
        provider: ProviderKind,
        access_token: impl Into<String>,
        refresh_token: Option<String>,
        client_id: Option<String>,
        client_secret: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        email: Option<String>,
        model: Option<String>,
    ) -> Self {
        let default_m = provider.default_model().to_string();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            provider,
            api_key: access_token.into(),
            model: model.unwrap_or(default_m),
            base_url: None,
            auth_type: AuthType::OAuth {
                refresh_token,
                client_id,
                client_secret,
                expires_at,
                email,
            },
            created_at: Utc::now(),
        }
    }

    pub fn is_oauth(&self) -> bool {
        matches!(self.auth_type, AuthType::OAuth { .. })
    }

    pub fn oauth_email(&self) -> Option<&str> {
        match &self.auth_type {
            AuthType::OAuth { email, .. } => email.as_deref(),
            _ => None,
        }
    }

    /// Mask the API key or show OAuth badge
    pub fn masked_key(&self) -> String {
        if let Some(email) = self.oauth_email() {
            return format!("OAuth: {}", email);
        }
        if self.is_oauth() {
            return "OAuth Token".to_string();
        }

        let key = &self.api_key;
        if key.len() <= 8 {
            "****".to_string()
        } else {
            let start = &key[..4];
            let end = &key[key.len() - 4..];
            format!("{}...{}", start, end)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: Role,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub account_label: Option<String>,
    pub provider: Option<ProviderKind>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::User,
            content: content.into(),
            timestamp: Utc::now(),
            account_label: None,
            provider: None,
        }
    }

    pub fn assistant(
        content: impl Into<String>,
        account_label: Option<String>,
        provider: Option<ProviderKind>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::Assistant,
            content: content.into(),
            timestamp: Utc::now(),
            account_label,
            provider,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::System,
            content: content.into(),
            timestamp: Utc::now(),
            account_label: None,
            provider: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Chunk(String),
    Finished,
    Error(String),
}
