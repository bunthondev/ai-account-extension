use serde_json::json;
use subagent::models::ProviderKind;
use subagent::web_auth::WebAuthServer;
use tokio::sync::mpsc::unbounded_channel;

#[tokio::test]
async fn test_web_auth_server_flow() {
    let (tx, mut rx) = unbounded_channel();
    let server = WebAuthServer::start(tx).expect("Failed to start WebAuthServer");
    let port = server.port;

    // Send HTTP GET / to verify web portal HTML
    let client = reqwest::Client::new();
    let get_resp = client
        .get(format!("http://127.0.0.1:{}", port))
        .send()
        .await
        .expect("Failed to send GET to web auth portal");

    assert!(get_resp.status().is_success());
    let html = get_resp.text().await.expect("Failed to read HTML body");
    assert!(html.contains("Sign into AI Account"));
    assert!(html.contains("Subagent AI Terminal"));

    // Send HTTP POST /api/signin with new account
    let payload = json!({
        "provider": "claude",
        "name": "Web Work Claude",
        "api_key": "sk-ant-api03-webtest12345",
        "model": "claude-3-5-sonnet-20241022"
    });

    let post_resp = client
        .post(format!("http://127.0.0.1:{}/api/signin", port))
        .json(&payload)
        .send()
        .await
        .expect("Failed to send POST to web auth endpoint");

    assert!(post_resp.status().is_success());

    // Verify channel received the account
    let received = rx.recv().await.expect("Did not receive account on channel");
    assert_eq!(received.name, "Web Work Claude");
    assert_eq!(received.provider, ProviderKind::AnthropicClaude);
    assert_eq!(received.api_key, "sk-ant-api03-webtest12345");
    assert_eq!(received.model, "claude-3-5-sonnet-20241022");

    server.stop();
}
