use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc::UnboundedSender, watch, Mutex};

use crate::models::{AccountProfile, ProviderKind};
use crate::oauth::google::{build_google_auth_url, exchange_code_for_token, fetch_user_info};

#[derive(Debug, Deserialize)]
pub struct WebSignInPayload {
    pub provider: String,
    pub name: String,
    pub api_key: String,
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
struct JsonResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Clone)]
pub struct PendingOAuth {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub model: String,
    pub state: String,
}

pub struct WebAuthServer {
    pub port: u16,
    shutdown_tx: watch::Sender<bool>,
}

impl WebAuthServer {
    pub fn start(account_sender: UnboundedSender<AccountProfile>) -> Result<Self> {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:8085")
            .or_else(|_| std::net::TcpListener::bind("127.0.0.1:0"))
            .context("Failed to bind local auth server")?;

        std_listener.set_nonblocking(true)?;
        let local_addr: SocketAddr = std_listener.local_addr()?;
        let port = local_addr.port();

        let listener = tokio::net::TcpListener::from_std(std_listener)
            .context("Failed to create tokio TcpListener")?;

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let account_sender = Arc::new(account_sender);
        let pending_oauth = Arc::new(Mutex::new(None::<PendingOAuth>));
        let http_client = Arc::new(Client::new());

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    accept_res = listener.accept() => {
                        if let Ok((mut socket, _)) = accept_res {
                            let tx = Arc::clone(&account_sender);
                            let oauth_store = Arc::clone(&pending_oauth);
                            let client = Arc::clone(&http_client);
                            tokio::spawn(async move {
                                let mut buf = [0u8; 8192];
                                if let Ok(n) = socket.read(&mut buf).await {
                                    if n == 0 {
                                        return;
                                    }
                                    let req = String::from_utf8_lossy(&buf[..n]);
                                    handle_request(&mut socket, &req, port, &tx, &oauth_store, &client).await;
                                }
                            });
                        }
                    }
                }
            }
        });

        // Open browser
        let url = format!("http://127.0.0.1:{}", port);
        open_browser(&url);

        Ok(Self { port, shutdown_tx })
    }

    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

pub fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn();
    }
}

async fn handle_request(
    socket: &mut tokio::net::TcpStream,
    req: &str,
    port: u16,
    account_sender: &Arc<UnboundedSender<AccountProfile>>,
    oauth_store: &Arc<Mutex<Option<PendingOAuth>>>,
    http_client: &Arc<Client>,
) {
    let mut lines = req.lines();
    let first_line = lines.next().unwrap_or_default();
    let parts: Vec<&str> = first_line.split_whitespace().collect();

    if parts.len() < 2 {
        let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
        return;
    }

    let method = parts[0];
    let full_path = parts[1];
    let (path, query) = if let Some(idx) = full_path.find('?') {
        (&full_path[..idx], &full_path[idx + 1..])
    } else {
        (full_path, "")
    };

    let query_map = parse_query(query);

    // 1. GET / -> Serve Main Portal
    if method == "GET" && path == "/" {
        let body = get_html_portal(port);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(resp.as_bytes()).await;
        return;
    }

    // 2. GET /oauth/google/start -> Redirect to Google OAuth consent
    if method == "GET" && path == "/oauth/google/start" {
        let client_id_input = query_map.get("client_id").map(|s| s.trim()).unwrap_or_default();
        let (client_id, client_secret) = if client_id_input.is_empty() || client_id_input == "default" {
            (
                crate::oauth::google::DEFAULT_GOOGLE_CLIENT_ID.to_string(),
                Some(crate::oauth::google::DEFAULT_GOOGLE_CLIENT_SECRET.to_string()),
            )
        } else {
            let secret = query_map
                .get("client_secret")
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string());
            (client_id_input.to_string(), secret)
        };
        let model = query_map.get("model").map(|s| s.as_str()).unwrap_or("gemini-2.5-flash").to_string();

        let state = uuid::Uuid::new_v4().to_string();
        let redirect_uri = format!("http://127.0.0.1:{}/oauth/callback", port);

        {
            let mut store = oauth_store.lock().await;
            *store = Some(PendingOAuth {
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
                model,
                state: state.clone(),
            });
        }

        let auth_url = build_google_auth_url(&client_id, &redirect_uri, &state);
        let resp = format!(
            "HTTP/1.1 302 Found\r\nLocation: {}\r\nConnection: close\r\n\r\n",
            auth_url
        );
        let _ = socket.write_all(resp.as_bytes()).await;
        return;
    }

    // 3. GET /oauth/callback -> Handle Google OAuth redirect
    if method == "GET" && path == "/oauth/callback" {
        if let Some(error) = query_map.get("error") {
            let err_html = error_html(&format!("Google returned an authentication error: {}", error));
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                err_html.len(),
                err_html
            );
            let _ = socket.write_all(resp.as_bytes()).await;
            return;
        }

        let code = match query_map.get("code") {
            Some(c) if !c.is_empty() => c.clone(),
            _ => {
                let err_html = error_html("No authorization code received from Google.");
                let resp = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    err_html.len(),
                    err_html
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }
        };

        let pending = {
            let mut store = oauth_store.lock().await;
            store.take()
        };

        let pending = match pending {
            Some(p) => {
                if let Some(incoming_state) = query_map.get("state") {
                    if incoming_state != &p.state {
                        let err_html = error_html("OAuth state mismatch. Request may have been intercepted.");
                        let resp = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            err_html.len(),
                            err_html
                        );
                        let _ = socket.write_all(resp.as_bytes()).await;
                        return;
                    }
                }
                p
            }
            None => {
                let err_html = error_html("Session expired or invalid state. Please try logging in again.");
                let resp = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    err_html.len(),
                    err_html
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }
        };

        let redirect_uri = format!("http://127.0.0.1:{}/oauth/callback", port);
        match exchange_code_for_token(
            http_client,
            &pending.client_id,
            pending.client_secret.as_deref(),
            &code,
            &redirect_uri,
        )
        .await
        {
            Ok(token_resp) => {
                let user_info = fetch_user_info(http_client, &token_resp.access_token).await.ok();
                let email = user_info.as_ref().and_then(|u| u.email.clone());
                let user_name = user_info.as_ref().and_then(|u| u.name.clone());

                let display_label = if let Some(e) = &email {
                    format!("Google ({})", e)
                } else if let Some(n) = &user_name {
                    format!("Google ({})", n)
                } else {
                    "Google Account (OAuth)".to_string()
                };

                let expires_at = token_resp
                    .expires_in
                    .map(|sec| Utc::now() + Duration::seconds(sec));

                let account = AccountProfile::new_oauth(
                    display_label,
                    ProviderKind::GoogleGemini,
                    token_resp.access_token,
                    token_resp.refresh_token,
                    Some(pending.client_id),
                    pending.client_secret,
                    expires_at,
                    email.clone(),
                    Some(pending.model),
                );

                let _ = account_sender.send(account);

                let success_html = success_html(email.as_deref().unwrap_or("Authenticated User"));
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    success_html.len(),
                    success_html
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }
            Err(e) => {
                let err_html = error_html(&format!("Failed to exchange token with Google: {}", e));
                let resp = format!(
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    err_html.len(),
                    err_html
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }
        }
    }

    // 4. POST /api/signin -> Manual API key sign-in
    if method == "POST" && path == "/api/signin" {
        let body_str = if let Some(idx) = req.find("\r\n\r\n") {
            &req[idx + 4..]
        } else if let Some(idx) = req.find("\n\n") {
            &req[idx + 2..]
        } else {
            ""
        };

        if let Ok(payload) = serde_json::from_str::<WebSignInPayload>(body_str.trim()) {
            let prov = match payload.provider.to_lowercase().as_str() {
                "gemini" | "google" => ProviderKind::GoogleGemini,
                "claude" | "anthropic" => ProviderKind::AnthropicClaude,
                "openai" | "codex" => ProviderKind::OpenAiCodex,
                _ => ProviderKind::GoogleGemini,
            };

            let name = if payload.name.trim().is_empty() {
                format!("{} Account", prov.display_name())
            } else {
                payload.name.trim().to_string()
            };

            let account = AccountProfile::new(
                name,
                prov,
                payload.api_key.trim(),
                payload.model.filter(|m| !m.trim().is_empty()),
                None,
            );

            let _ = account_sender.send(account);

            let json_resp = serde_json::to_string(&JsonResponse {
                success: true,
                message: "Account authenticated successfully! You may now return to your terminal.".to_string(),
            }).unwrap_or_default();

            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json_resp.len(),
                json_resp
            );
            let _ = socket.write_all(resp.as_bytes()).await;
            return;
        } else {
            let json_resp = serde_json::to_string(&JsonResponse {
                success: false,
                message: "Invalid JSON payload or missing fields.".to_string(),
            }).unwrap_or_default();

            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                json_resp.len(),
                json_resp
            );
            let _ = socket.write_all(resp.as_bytes()).await;
            return;
        }
    }

    let _ = socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if q.is_empty() {
        return map;
    }
    for pair in q.split('&') {
        let mut kv = pair.splitn(2, '=');
        if let Some(key) = kv.next() {
            let val = kv.next().unwrap_or_default();
            map.insert(urldecode(key), urldecode(val));
        }
    }
    map
}

fn urldecode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                result.push(val);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            result.push(b' ');
            i += 1;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

fn success_html(user_id: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Sign-in Successful</title>
  <style>
    body {{
      background: #0b0f19; color: #f8fafc; font-family: -apple-system, sans-serif;
      display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0;
    }}
    .card {{
      background: #151c2c; border: 1px solid #10b981; border-radius: 16px;
      padding: 3rem; text-align: center; max-width: 480px; box-shadow: 0 20px 40px rgba(0,0,0,0.5);
    }}
    .icon {{ font-size: 3.5rem; margin-bottom: 1rem; color: #10b981; }}
    h1 {{ margin-bottom: 0.5rem; font-size: 1.5rem; }}
    p {{ color: #94a3b8; font-size: 1rem; line-height: 1.5; }}
    .user {{ color: #38bdf8; font-weight: 700; }}
  </style>
</head>
<body>
  <div class="card">
    <div class="icon">✓</div>
    <h1>Authentication Successful!</h1>
    <p>Signed in as <span class="user">{}</span></p>
    <p style="margin-top: 1rem;">Your account is now linked and active in <strong>Subagent Terminal</strong>.<br>You can safely close this window.</p>
  </div>
  <script>setTimeout(() => window.close(), 4000);</script>
</body>
</html>"#,
        user_id
    )
}

fn error_html(err_msg: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Authentication Error</title>
  <style>
    body {{
      background: #0b0f19; color: #f8fafc; font-family: -apple-system, sans-serif;
      display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0;
    }}
    .card {{
      background: #151c2c; border: 1px solid #ef4444; border-radius: 16px;
      padding: 3rem; text-align: center; max-width: 480px; box-shadow: 0 20px 40px rgba(0,0,0,0.5);
    }}
    .icon {{ font-size: 3.5rem; margin-bottom: 1rem; color: #ef4444; }}
    h1 {{ margin-bottom: 0.5rem; font-size: 1.5rem; color: #f87171; }}
    p {{ color: #94a3b8; font-size: 1rem; line-height: 1.5; }}
    a {{ color: #38bdf8; text-decoration: none; font-weight: bold; margin-top: 1.5rem; display: inline-block; }}
  </style>
</head>
<body>
  <div class="card">
    <div class="icon">✕</div>
    <h1>Sign-in Failed</h1>
    <p>{}</p>
    <a href="/">← Return to Sign In Portal</a>
  </div>
</body>
</html>"#,
        err_msg
    )
}

fn get_html_portal(port: u16) -> String {
    let template = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Subagent AI Terminal — Web Sign In</title>
  <style>
    :root {
      --bg: #0b0f19;
      --card-bg: #151c2c;
      --border: #232f48;
      --accent: #38bdf8;
      --accent-hover: #0ea5e9;
      --google: #4285f4;
      --text: #f8fafc;
      --text-muted: #94a3b8;
      --success: #10b981;
      --error: #ef4444;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      background-color: var(--bg);
      color: var(--text);
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 1.5rem;
    }
    .container {
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 16px;
      padding: 2.5rem;
      max-width: 540px;
      width: 100%;
      box-shadow: 0 20px 40px rgba(0, 0, 0, 0.5);
    }
    .header {
      text-align: center;
      margin-bottom: 1.75rem;
    }
    .badge {
      display: inline-block;
      font-size: 0.75rem;
      text-transform: uppercase;
      letter-spacing: 0.1em;
      padding: 0.35rem 0.75rem;
      border-radius: 9999px;
      background: rgba(56, 189, 248, 0.15);
      color: var(--accent);
      margin-bottom: 0.75rem;
      font-weight: 600;
    }
    h1 { font-size: 1.6rem; font-weight: 700; color: #fff; margin-bottom: 0.5rem; }
    p.subtitle { font-size: 0.95rem; color: var(--text-muted); }

    .nav-tabs {
      display: flex;
      border-bottom: 1px solid var(--border);
      margin-bottom: 1.75rem;
    }
    .tab-btn {
      flex: 1;
      background: transparent;
      border: none;
      border-bottom: 2px solid transparent;
      padding: 0.75rem 0.5rem;
      color: var(--text-muted);
      font-weight: 600;
      font-size: 0.95rem;
      cursor: pointer;
      text-align: center;
      transition: all 0.2s;
    }
    .tab-btn.active {
      color: var(--accent);
      border-bottom-color: var(--accent);
    }

    .tab-content { display: none; }
    .tab-content.active { display: block; }

    .google-btn {
      width: 100%;
      background: #ffffff;
      color: #3c4043;
      border: 1px solid #dadce0;
      border-radius: 8px;
      padding: 0.85rem 1rem;
      font-size: 1rem;
      font-weight: 600;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 0.75rem;
      cursor: pointer;
      transition: all 0.2s;
      margin-bottom: 1.25rem;
    }
    .google-btn:hover {
      background: #f8f9fa;
      box-shadow: 0 2px 6px rgba(0,0,0,0.2);
    }
    .google-icon {
      width: 20px;
      height: 20px;
    }

    .form-group { margin-bottom: 1.25rem; }
    label { display: block; font-size: 0.85rem; font-weight: 500; margin-bottom: 0.4rem; color: var(--text-muted); }
    input, select {
      width: 100%;
      background: #0d1322;
      border: 1px solid var(--border);
      border-radius: 8px;
      padding: 0.75rem 1rem;
      font-size: 0.95rem;
      color: #fff;
    }
    input:focus, select:focus { outline: none; border-color: var(--accent); }

    .submit-btn {
      width: 100%;
      background: var(--accent);
      color: #0b0f19;
      border: none;
      border-radius: 8px;
      padding: 0.85rem;
      font-size: 1rem;
      font-weight: 700;
      cursor: pointer;
      margin-top: 1rem;
      transition: background 0.2s;
    }
    .submit-btn:hover { background: var(--accent-hover); }

    .helper-box {
      background: rgba(30, 41, 59, 0.6);
      border: 1px dashed var(--border);
      border-radius: 8px;
      padding: 1rem;
      margin-top: 1.25rem;
      font-size: 0.85rem;
      color: var(--text-muted);
      line-height: 1.5;
    }
    .helper-box strong { color: #fff; }
    .helper-box a { color: var(--accent); text-decoration: none; }

    .provider-grid {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 0.75rem;
      margin-bottom: 1.25rem;
    }
    .provider-btn {
      background: #1c263c;
      border: 2px solid var(--border);
      color: var(--text);
      padding: 0.85rem 0.5rem;
      border-radius: 10px;
      cursor: pointer;
      font-weight: 600;
      font-size: 0.85rem;
      text-align: center;
      transition: all 0.2s;
    }
    .provider-btn.active {
      border-color: var(--accent);
      background: rgba(56, 189, 248, 0.1);
      color: var(--accent);
    }
    .status-msg {
      margin-top: 1.25rem;
      padding: 1rem;
      border-radius: 8px;
      text-align: center;
      display: none;
    }
    .status-msg.success { display: block; background: rgba(16, 185, 129, 0.15); border: 1px solid var(--success); color: #34d399; }
    .status-msg.error { display: block; background: rgba(239, 68, 68, 0.15); border: 1px solid var(--error); color: #f87171; }
  </style>
</head>
<body>
  <div class="container">
    <div class="header">
      <span class="badge">Authentication Portal</span>
      <h1>Sign into AI Account</h1>
      <p class="subtitle">Authenticate account to sync into Subagent Terminal</p>
    </div>

    <div class="nav-tabs">
      <button class="tab-btn active" id="tabOAuth" onclick="switchTab('oauth')">Google OAuth 2.0</button>
      <button class="tab-btn" id="tabApiKey" onclick="switchTab('apikey')">API Key / Token</button>
    </div>

    <!-- 1. Google OAuth Tab -->
    <div id="contentOAuth" class="tab-content active">
      <form id="googleOAuthForm">
        <div class="form-group">
          <label for="googleModel">Default Gemini Model</label>
          <select id="googleModel">
            <option value="gemini-2.5-flash" selected>gemini-2.5-flash (Fast & multimodal)</option>
            <option value="gemini-1.5-pro">gemini-1.5-pro (High intelligence)</option>
            <option value="gemini-1.5-flash">gemini-1.5-flash</option>
          </select>
        </div>

        <button type="submit" class="google-btn">
          <svg class="google-icon" viewBox="0 0 24 24">
            <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
            <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
            <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22.81-.63z"/>
            <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z"/>
          </svg>
          Sign in with Google Account (1-Click)
        </button>

        <div style="text-align: center; margin: 1.25rem 0; color: var(--text-muted); font-size: 0.8rem; letter-spacing: 0.05em;">
          ─── OR USE YOUR GOOGLE ACCOUNT AT AI STUDIO ───
        </div>

        <a href="https://aistudio.google.com/app/apikey" target="_blank" style="display: block; width: 100%; text-align: center; padding: 0.8rem; border-radius: 8px; background: rgba(56, 189, 248, 0.1); border: 1px solid var(--accent); color: var(--accent); text-decoration: none; font-weight: 600; font-size: 0.9rem;">
          ✨ Get Free Gemini Key with your Google Account
        </a>

        <details style="margin-top: 1.5rem; font-size: 0.85rem; color: var(--text-muted);">
          <summary style="cursor: pointer; font-weight: 600; color: #fff;">Custom Google Cloud OAuth Client ID (Optional)</summary>
          <div style="margin-top: 0.75rem;">
            <div class="form-group">
              <label for="googleClientId">Custom Client ID</label>
              <input type="text" id="googleClientId" placeholder="Leave empty for default 1-click Google auth" />
            </div>
            <div class="form-group">
              <label for="googleClientSecret">Custom Client Secret</label>
              <input type="password" id="googleClientSecret" placeholder="Optional" />
            </div>
          </div>
        </details>
      </form>
    </div>

    <!-- 2. API Key Tab -->
    <div id="contentApiKey" class="tab-content">
      <form id="authForm">
        <label>Select AI Provider</label>
        <div class="provider-grid">
          <button type="button" class="provider-btn active" data-provider="gemini" data-default-model="gemini-2.5-flash">Google Gemini</button>
          <button type="button" class="provider-btn" data-provider="claude" data-default-model="claude-3-5-sonnet-20241022">Anthropic Claude</button>
          <button type="button" class="provider-btn" data-provider="openai" data-default-model="gpt-4o">OpenAI Codex</button>
        </div>

        <div class="form-group">
          <label for="accName">Account Label / Name</label>
          <input type="text" id="accName" value="Google Gemini Account" required />
        </div>

        <div class="form-group">
          <label for="apiKey">API Key / Access Token</label>
          <input type="password" id="apiKey" placeholder="Enter provider API key" required />
        </div>

        <div class="form-group">
          <label for="model">Default Model</label>
          <input type="text" id="model" value="gemini-2.5-flash" />
        </div>

        <button type="submit" class="submit-btn" id="submitBtn">Connect to Terminal</button>
        <div id="statusBox" class="status-msg"></div>
      </form>
    </div>
  </div>

  <script>
    function switchTab(tab) {
      document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
      document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
      if (tab === 'oauth') {
        document.getElementById('tabOAuth').classList.add('active');
        document.getElementById('contentOAuth').classList.add('active');
      } else {
        document.getElementById('tabApiKey').classList.add('active');
        document.getElementById('contentApiKey').classList.add('active');
      }
    }

    // Load saved client ID from localStorage
    const savedClientId = localStorage.getItem('google_client_id');
    if (savedClientId) {
      document.getElementById('googleClientId').value = savedClientId;
    }

    document.getElementById('googleOAuthForm').addEventListener('submit', (e) => {
      e.preventDefault();
      const clientIdInput = document.getElementById('googleClientId').value.trim();
      const clientId = clientIdInput || 'default';
      const clientSecret = document.getElementById('googleClientSecret').value.trim();
      const model = document.getElementById('googleModel').value;

      if (clientIdInput) {
        localStorage.setItem('google_client_id', clientIdInput);
      }
      const url = '/oauth/google/start?client_id=' + encodeURIComponent(clientId) + '&client_secret=' + encodeURIComponent(clientSecret) + '&model=' + encodeURIComponent(model);
      window.location.href = url;
    });

    // Manual API key handler
    let currentProvider = "gemini";
    const providerBtns = document.querySelectorAll(".provider-btn");
    const accNameInput = document.getElementById("accName");
    const modelInput = document.getElementById("model");
    const apiKeyInput = document.getElementById("apiKey");
    const statusBox = document.getElementById("statusBox");
    const submitBtn = document.getElementById("submitBtn");

    providerBtns.forEach(btn => {
      btn.addEventListener("click", () => {
        providerBtns.forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        currentProvider = btn.dataset.provider;
        modelInput.value = btn.dataset.defaultModel;
        accNameInput.value = (currentProvider === "gemini" ? "Google Gemini" : currentProvider === "claude" ? "Anthropic Claude" : "OpenAI Codex") + " Account";
      });
    });

    document.getElementById("authForm").addEventListener("submit", async (e) => {
      e.preventDefault();
      statusBox.className = "status-msg";
      statusBox.style.display = "none";
      submitBtn.disabled = true;
      submitBtn.textContent = "Connecting...";

      try {
        const res = await fetch("/api/signin", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            provider: currentProvider,
            name: accNameInput.value,
            api_key: apiKeyInput.value,
            model: modelInput.value
          })
        });

        const data = await res.json();
        if (res.ok && data.success) {
          statusBox.textContent = "✔ " + data.message;
          statusBox.className = "status-msg success";
          submitBtn.textContent = "Connected!";
          setTimeout(() => window.close(), 3000);
        } else {
          statusBox.textContent = "⚠ " + (data.message || "Failed to authenticate.");
          statusBox.className = "status-msg error";
          submitBtn.disabled = false;
          submitBtn.textContent = "Connect to Terminal";
        }
      } catch (err) {
        statusBox.textContent = "⚠ Connection error: " + err.message;
        statusBox.className = "status-msg error";
        submitBtn.disabled = false;
        submitBtn.textContent = "Connect to Terminal";
      }
    });
  </script>
</body>
</html>"##;

    template.replace("__PORT__", &port.to_string())
}
