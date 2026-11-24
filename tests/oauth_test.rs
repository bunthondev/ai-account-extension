use subagent::models::{AccountProfile, AuthType, ProviderKind};
use subagent::oauth::google::build_google_auth_url;
use subagent::web_auth::WebAuthServer;
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn test_google_auth_url_construction() {
    let client_id = "test-client-123.apps.googleusercontent.com";
    let redirect_uri = "http://127.0.0.1:8085/oauth/callback";
    let state = "test-random-state";

    let url = build_google_auth_url(client_id, redirect_uri, state);
    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains("client_id=test-client-123.apps.googleusercontent.com"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8085%2Foauth%2Fcallback"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("access_type=offline"));
    assert!(url.contains("prompt=consent"));
    assert!(url.contains("state=test-random-state"));
}

#[test]
fn test_oauth_account_profile() {
    let acc = AccountProfile::new_oauth(
        "Google (user@gmail.com)",
        ProviderKind::GoogleGemini,
        "ya29.a0AfH6SMTestAccessToken123",
        Some("1//04testRefreshToken".to_string()),
        Some("client-id-123".to_string()),
        None,
        None,
        Some("user@gmail.com".to_string()),
        Some("gemini-2.5-flash".to_string()),
    );

    assert!(acc.is_oauth());
    assert_eq!(acc.oauth_email(), Some("user@gmail.com"));
    assert_eq!(acc.masked_key(), "OAuth: user@gmail.com");

    match &acc.auth_type {
        AuthType::OAuth { refresh_token, client_id, .. } => {
            assert_eq!(refresh_token.as_deref(), Some("1//04testRefreshToken"));
            assert_eq!(client_id.as_deref(), Some("client-id-123"));
        }
        _ => panic!("Expected OAuth auth_type"),
    }
}

#[tokio::test]
async fn test_oauth_start_endpoint_redirect() {
    let (tx, _rx) = unbounded_channel();
    let server = WebAuthServer::start(tx).expect("Failed to start WebAuthServer");
    let port = server.port;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none()) // do not follow redirect
        .build()
        .unwrap();

    let start_url = format!(
        "http://127.0.0.1:{}/oauth/google/start?client_id=my-test-client.apps.googleusercontent.com&model=gemini-2.5-flash",
        port
    );

    let resp = client.get(&start_url).send().await.unwrap();

    assert_eq!(resp.status().as_u16(), 302);
    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(location.contains("client_id=my-test-client.apps.googleusercontent.com"));
    assert!(location.contains(&format!("redirect_uri=http%3A%2F%2F127.0.0.1%3A{}%2Foauth%2Fcallback", port)));

    server.stop();
}
