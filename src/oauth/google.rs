use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub const DEFAULT_GOOGLE_CLIENT_ID: &str =
    "764086051850-6qr4p6gpi6hn506pt8ejuq83di341hur.apps.googleusercontent.com";
pub const DEFAULT_GOOGLE_CLIENT_SECRET: &str =
    "d-FL95Q19q7MQmFpd7hHD0Ty";

#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GoogleUserInfo {
    pub id: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
}

pub fn build_google_auth_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    let scopes = [
        "https://www.googleapis.com/auth/cloud-platform",
        "https://www.googleapis.com/auth/userinfo.email",
        "https://www.googleapis.com/auth/userinfo.profile",
        "openid",
    ]
    .join(" ");

    format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
client_id={}&\
redirect_uri={}&\
response_type=code&\
scope={}&\
access_type=offline&\
prompt=consent&\
state={}",
        urlencoding(client_id),
        urlencoding(redirect_uri),
        urlencoding(&scopes),
        urlencoding(state),
    )
}

pub async fn exchange_code_for_token(
    client: &Client,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    redirect_uri: &str,
) -> Result<GoogleTokenResponse> {
    let mut params = vec![
        ("code", code),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];

    let secret = client_secret.unwrap_or_default();
    if !secret.is_empty() {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .context("Failed to exchange code with Google OAuth endpoint")?;

    if !resp.status().is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Google OAuth token error: {}", err_text));
    }

    let token_resp = resp
        .json::<GoogleTokenResponse>()
        .await
        .context("Failed to parse Google OAuth token response")?;

    Ok(token_resp)
}

pub async fn fetch_user_info(client: &Client, access_token: &str) -> Result<GoogleUserInfo> {
    let resp = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .context("Failed to fetch Google user info")?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Google UserInfo error: {}", err));
    }

    let info = resp
        .json::<GoogleUserInfo>()
        .await
        .context("Failed to parse Google user info response")?;

    Ok(info)
}

#[allow(dead_code)]
pub async fn refresh_access_token(
    client: &Client,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<(String, Option<chrono::DateTime<Utc>>)> {
    let mut params = vec![
        ("client_id", client_id),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];

    let secret = client_secret.unwrap_or_default();
    if !secret.is_empty() {
        params.push(("client_secret", secret));
    }

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .context("Failed to refresh Google OAuth token")?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Google token refresh failed: {}", err));
    }

    let token_resp = resp.json::<GoogleTokenResponse>().await?;
    let expires_at = token_resp
        .expires_in
        .map(|sec| Utc::now() + Duration::seconds(sec));

    Ok((token_resp.access_token, expires_at))
}

fn urlencoding(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}
