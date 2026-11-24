use subagent::config::AppConfig;
use subagent::models::{AccountProfile, ProviderKind, ChatMessage, Role};
use subagent::providers::create_client;

#[test]
fn test_account_creation_and_masking() {
    let gemini_acc = AccountProfile::new(
        "Personal Gemini",
        ProviderKind::GoogleGemini,
        "AIzaSyDemoKey1234567890",
        None,
        None,
    );
    assert_eq!(gemini_acc.provider, ProviderKind::GoogleGemini);
    assert_eq!(gemini_acc.model, "gemini-2.5-flash");
    assert_eq!(gemini_acc.masked_key(), "AIza...7890");

    let claude_acc = AccountProfile::new(
        "Work Claude",
        ProviderKind::AnthropicClaude,
        "sk-ant-api03-abcdefghijklmn",
        Some("claude-3-5-sonnet-20241022".to_string()),
        None,
    );
    assert_eq!(claude_acc.provider, ProviderKind::AnthropicClaude);
    assert_eq!(claude_acc.model, "claude-3-5-sonnet-20241022");
    assert_eq!(claude_acc.masked_key(), "sk-a...klmn");

    let openai_acc = AccountProfile::new(
        "Codex OpenAI",
        ProviderKind::OpenAiCodex,
        "sk-proj-xyz1234567890",
        Some("gpt-4o".to_string()),
        None,
    );
    assert_eq!(openai_acc.provider, ProviderKind::OpenAiCodex);
    assert_eq!(openai_acc.model, "gpt-4o");
    assert_eq!(openai_acc.masked_key(), "sk-p...7890");
}

#[test]
fn test_config_multi_account_management() {
    let mut config = AppConfig::default();
    assert!(config.active_account().is_none());

    let acc1 = AccountProfile::new("Gemini Free", ProviderKind::GoogleGemini, "key-1", None, None);
    let acc2 = AccountProfile::new("Gemini Pro", ProviderKind::GoogleGemini, "key-2", None, None);
    let acc3 = AccountProfile::new("Claude Team", ProviderKind::AnthropicClaude, "key-3", None, None);

    let id1 = acc1.id.clone();
    let id2 = acc2.id.clone();
    let id3 = acc3.id.clone();

    config.accounts.push(acc1);
    config.accounts.push(acc2);
    config.accounts.push(acc3);
    config.active_account_id = Some(id1.clone());

    // First account is active
    assert_eq!(config.active_account().unwrap().name, "Gemini Free");

    // Switch to second account
    config.set_active_account(&id2).unwrap();
    assert_eq!(config.active_account().unwrap().name, "Gemini Pro");

    // Switch to Claude account
    config.set_active_account(&id3).unwrap();
    assert_eq!(config.active_account().unwrap().name, "Claude Team");

    // Filter accounts by provider
    let gemini_accounts = config.accounts_for_provider(ProviderKind::GoogleGemini);
    assert_eq!(gemini_accounts.len(), 2);

    let claude_accounts = config.accounts_for_provider(ProviderKind::AnthropicClaude);
    assert_eq!(claude_accounts.len(), 1);

    let openai_accounts = config.accounts_for_provider(ProviderKind::OpenAiCodex);
    assert_eq!(openai_accounts.len(), 0);
}

#[test]
fn test_provider_client_factory() {
    let acc_gemini = AccountProfile::new("Gemini", ProviderKind::GoogleGemini, "test-key", None, None);
    let acc_claude = AccountProfile::new("Claude", ProviderKind::AnthropicClaude, "test-key", None, None);
    let acc_openai = AccountProfile::new("OpenAI", ProviderKind::OpenAiCodex, "test-key", None, None);

    let _c1 = create_client(&acc_gemini);
    let _c2 = create_client(&acc_claude);
    let _c3 = create_client(&acc_openai);
}

#[test]
fn test_chat_messages_construction() {
    let u = ChatMessage::user("Hello AI");
    assert_eq!(u.role, Role::User);
    assert_eq!(u.content, "Hello AI");

    let a = ChatMessage::assistant("Hello human", Some("Work Claude".to_string()), Some(ProviderKind::AnthropicClaude));
    assert_eq!(a.role, Role::Assistant);
    assert_eq!(a.account_label.as_deref(), Some("Work Claude"));
    assert_eq!(a.provider, Some(ProviderKind::AnthropicClaude));

    let s = ChatMessage::system("System instruction");
    assert_eq!(s.role, Role::System);
}
