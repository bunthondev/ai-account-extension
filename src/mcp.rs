use std::io::{self, BufRead};
use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::mpsc::unbounded_channel;

use crate::config::AppConfig;
use crate::models::{AccountProfile, ChatMessage, ProviderKind, StreamEvent};
use crate::providers::create_client;

pub async fn run_stdio_mcp_server() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handle = stdin.lock();

    let mut line = String::new();
    while handle.read_line(&mut line)? > 0 {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            if let Ok(req) = serde_json::from_str::<Value>(trimmed) {
                if let Some(resp) = handle_mcp_request(req).await {
                    let mut out = stdout.lock();
                    use std::io::Write;
                    writeln!(out, "{}", serde_json::to_string(&resp)?)?;
                    out.flush()?;
                }
            }
        }
        line.clear();
    }
    Ok(())
}

async fn handle_mcp_request(req: Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or_default();

    match method {
        "initialize" => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "multi-account-ai",
                        "version": "0.1.0"
                    }
                }
            }))
        }
        "notifications/initialized" | "initialized" => None,
        "ping" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        })),
        "tools/list" => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "list_ai_accounts",
                            "description": "List all configured AI accounts (Google Gemini, Anthropic Claude, OpenAI) and see which account is currently active.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        },
                        {
                            "name": "switch_ai_account",
                            "description": "Switch the active AI account to a specified account by name or ID.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "target": {
                                        "type": "string",
                                        "description": "The account name, email, or ID to activate (e.g. 'Work Gemini', 'Claude', 'user@gmail.com')"
                                    }
                                },
                                "required": ["target"]
                            }
                        },
                        {
                            "name": "add_ai_account",
                            "description": "Add a new AI account profile for Gemini, Claude, or OpenAI.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "name": {
                                        "type": "string",
                                        "description": "Account label (e.g. 'Personal Gemini')"
                                    },
                                    "provider": {
                                        "type": "string",
                                        "enum": ["gemini", "claude", "openai"],
                                        "description": "AI provider service"
                                    },
                                    "api_key": {
                                        "type": "string",
                                        "description": "The provider API key or access token"
                                    },
                                    "model": {
                                        "type": "string",
                                        "description": "Optional model override"
                                    }
                                },
                                "required": ["name", "provider", "api_key"]
                            }
                        },
                        {
                            "name": "query_ai_model",
                            "description": "Send a prompt to the active account or specified account and get the completion response.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "prompt": {
                                        "type": "string",
                                        "description": "The message or prompt to send to the model"
                                    },
                                    "account": {
                                        "type": "string",
                                        "description": "Optional account name to use. If omitted, uses the currently active account."
                                    }
                                },
                                "required": ["prompt"]
                            }
                        },
                        {
                            "name": "open_web_signin",
                            "description": "Launch the local web authentication portal in the user's browser to connect an account via Google OAuth or API key.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        }
                    ]
                }
            }))
        }
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            let result = execute_tool(tool_name, args).await;
            match result {
                Ok(content) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": content
                            }
                        ]
                    }
                })),
                Err(e) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32000,
                        "message": e.to_string()
                    }
                })),
            }
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method '{}' not found", method)
            }
        })),
    }
}

async fn execute_tool(name: &str, args: Value) -> Result<String> {
    let mut config = AppConfig::load();

    match name {
        "list_ai_accounts" => {
            if config.accounts.is_empty() {
                return Ok("No accounts configured yet. Use `add_ai_account` or `open_web_signin` to add an account.".to_string());
            }

            let mut out = String::from("| Active | Account Name | Provider | Model | Key / Auth |\n");
            out.push_str("|:---:|:---|:---|:---|:---|\n");

            for acc in &config.accounts {
                let is_active = config.active_account_id.as_deref() == Some(&acc.id);
                let active_badge = if is_active { "●" } else { " " };
                out.push_str(&format!(
                    "| {} | **{}** | {} | `{}` | `{}` |\n",
                    active_badge,
                    acc.name,
                    acc.provider.display_name(),
                    acc.model,
                    acc.masked_key()
                ));
            }
            Ok(out)
        }
        "switch_ai_account" => {
            let target = args.get("target").and_then(|t| t.as_str()).unwrap_or_default().trim();
            if target.is_empty() {
                return Err(anyhow::anyhow!("Target account name or ID cannot be empty"));
            }

            let found = config.accounts.iter().find(|a| {
                a.id == target
                    || a.name.eq_ignore_ascii_case(target)
                    || a.oauth_email().map(|e| e.eq_ignore_ascii_case(target)).unwrap_or(false)
            });

            if let Some(acc) = found {
                let id = acc.id.clone();
                let name = acc.name.clone();
                let prov = acc.provider.display_name();
                let model = acc.model.clone();
                config.set_active_account(&id)?;
                Ok(format!("Switched active account to: **{}** ({}) using model `{}`", name, prov, model))
            } else {
                Err(anyhow::anyhow!("No account found matching '{}'. Use `list_ai_accounts` to see available accounts.", target))
            }
        }
        "add_ai_account" => {
            let acc_name = args.get("name").and_then(|n| n.as_str()).unwrap_or_default().trim();
            let prov_str = args.get("provider").and_then(|p| p.as_str()).unwrap_or_default().trim();
            let api_key = args.get("api_key").and_then(|k| k.as_str()).unwrap_or_default().trim();
            let model = args.get("model").and_then(|m| m.as_str()).filter(|s| !s.trim().is_empty());

            if acc_name.is_empty() {
                return Err(anyhow::anyhow!("Account name is required"));
            }
            if api_key.is_empty() {
                return Err(anyhow::anyhow!("API key is required"));
            }

            let provider = match prov_str.to_lowercase().as_str() {
                "gemini" | "google" => ProviderKind::GoogleGemini,
                "claude" | "anthropic" => ProviderKind::AnthropicClaude,
                "openai" | "codex" => ProviderKind::OpenAiCodex,
                _ => return Err(anyhow::anyhow!("Invalid provider '{}'. Must be 'gemini', 'claude', or 'openai'.", prov_str)),
            };

            let account = AccountProfile::new(
                acc_name,
                provider,
                api_key,
                model.map(|s| s.to_string()),
                None,
            );

            let added_name = account.name.clone();
            config.add_account(account)?;
            Ok(format!("Successfully created and saved account profile: **{}** ({})", added_name, provider.display_name()))
        }
        "query_ai_model" => {
            let prompt = args.get("prompt").and_then(|p| p.as_str()).unwrap_or_default().trim();
            if prompt.is_empty() {
                return Err(anyhow::anyhow!("Prompt cannot be empty"));
            }

            let account = if let Some(target) = args.get("account").and_then(|a| a.as_str()) {
                config.accounts.iter().find(|a| {
                    a.id == target || a.name.eq_ignore_ascii_case(target)
                }).cloned()
            } else {
                config.active_account().cloned()
            };

            let account = match account {
                Some(a) => a,
                None => return Err(anyhow::anyhow!("No active account available. Please add or select an account first.")),
            };

            let client = create_client(&account);
            let history = vec![ChatMessage::user(prompt)];
            let (tx, mut rx) = unbounded_channel();

            tokio::spawn(async move {
                let _ = client.stream_completion(&history, tx).await;
            });

            let mut output = String::new();
            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Chunk(chunk) => output.push_str(&chunk),
                    StreamEvent::Finished => break,
                    StreamEvent::Error(err) => return Err(anyhow::anyhow!("Streaming error: {}", err)),
                }
            }

            Ok(output)
        }
        "open_web_signin" => {
            let (tx, mut rx) = unbounded_channel();
            let server = crate::web_auth::WebAuthServer::start(tx)?;
            let port = server.port;

            tokio::spawn(async move {
                if let Some(account) = rx.recv().await {
                    let mut conf = AppConfig::load();
                    let _ = conf.add_account(account);
                }
                server.stop();
            });

            Ok(format!("Web Sign-In portal opened at http://127.0.0.1:{}. Complete your sign-in in the browser window.", port))
        }
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}
