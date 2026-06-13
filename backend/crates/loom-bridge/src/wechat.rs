//! WechatAdapter — polls iLink API for WeChat messages.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::*;

const ILINK_BASE: &str = "https://api.ilink.ai/api/v1";

/// Backoff bounds for the poll loop when the API errors.
const BACKOFF_BASE: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// Steady-state delay between successful polls.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// iLink API key wrapper whose `Debug`/`Display` never reveal the secret.
/// Mirrors the Telegram `BotToken` so the key cannot leak via logs.
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    fn new(key: String) -> Self {
        Self(key)
    }

    /// Reveal the raw key. ONLY for building the Authorization header — never log.
    fn reveal(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(***)")
    }
}

impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

pub struct WechatAdapter {
    api_key: ApiKey,
    client: reqwest::Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    #[allow(dead_code)]
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    poll_handle: Option<JoinHandle<()>>,
}

impl WechatAdapter {
    pub fn new(api_key: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            api_key: ApiKey::new(api_key),
            client: reqwest::Client::new(),
            health: AdapterHealth::Disconnected,
            rx,
            tx,
            abort: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key.reveal())
    }

    /// Validate credentials by issuing one authenticated poll request and
    /// rejecting on auth failures (401/403). iLink exposes no dedicated
    /// identity endpoint, so this is the feasible "validate before Connected"
    /// analog: a 401/403 means the key is bad and we should not poll.
    async fn validate(&self) -> Result<()> {
        let url = format!("{ILINK_BASE}/messages/poll");
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(anyhow::anyhow!(
                "iLink rejected the API key (HTTP {status})"
            ));
        }
        Ok(())
    }

    fn parse_ilink_message(msg: &serde_json::Value) -> Option<BridgeMessage> {
        // Untrusted, arbitrary-length content from a remote user: cap at ingest.
        let content = MessageContent::Text(cap_inbound_text(msg.get("content")?.as_str()?));
        Some(BridgeMessage {
            platform: Platform::Wechat,
            chat_id: msg.get("chat_id")?.as_str()?.to_string(),
            sender_id: format!("wx-{}", msg.get("sender_id")?.as_str()?),
            sender_name: msg
                .get("sender_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            content,
            reply_to: None,
            external_message_id: msg.get("message_id")?.as_str()?.to_string(),
            timestamp: chrono::Utc::now(),
            // Inbound from an external WeChat user: always untrusted.
            untrusted: true,
        })
    }
}

#[async_trait]
impl ChannelAdapter for WechatAdapter {
    fn platform(&self) -> Platform {
        Platform::Wechat
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;

        // Validate credentials BEFORE transitioning to Connected. On an auth
        // failure, record the error health and surface it — do not start polling.
        if let Err(e) = self.validate().await {
            let msg = e.to_string();
            self.health = AdapterHealth::Error(msg.clone());
            tracing::error!("WeChat (iLink) connect failed: {msg}");
            return Err(e);
        }

        self.abort.store(false, Ordering::SeqCst);
        let api_key = self.api_key.clone();
        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();

        let handle = tokio::spawn(async move {
            let mut since_id = String::new();
            let mut backoff = BACKOFF_BASE;
            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }
                let mut url = format!("{ILINK_BASE}/messages/poll");
                if !since_id.is_empty() {
                    // `since_id` is a message cursor, not a secret.
                    url.push_str(&format!("?since_id={since_id}"));
                }
                match client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", api_key.reveal()))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let status = resp.status();
                        // Auth/permission errors are not transient — back off.
                        if status == reqwest::StatusCode::UNAUTHORIZED
                            || status == reqwest::StatusCode::FORBIDDEN
                        {
                            tracing::warn!(
                                "iLink poll HTTP {status}; backing off {:?}",
                                backoff
                            );
                            sleep_with_abort(backoff, &abort).await;
                            backoff = next_backoff(backoff);
                            continue;
                        }
                        match resp.json::<serde_json::Value>().await {
                            Ok(body) => {
                                backoff = BACKOFF_BASE; // healthy: reset
                                if let Some(messages) = body["messages"].as_array() {
                                    for msg in messages {
                                        if let Some(bm) = Self::parse_ilink_message(msg)
                                            && tx.send(bm).await.is_err()
                                        {
                                            tracing::warn!(
                                                "WeChat inbound channel closed; stopping poll"
                                            );
                                            return;
                                        }
                                        // Advance the cursor only AFTER the
                                        // message is durably enqueued.
                                        if let Some(id) = msg["message_id"].as_str() {
                                            since_id = id.to_string();
                                        }
                                    }
                                }
                                sleep_with_abort(POLL_INTERVAL, &abort).await;
                            }
                            Err(e) => {
                                tracing::warn!("iLink poll decode failed: {e}");
                                sleep_with_abort(backoff, &abort).await;
                                backoff = next_backoff(backoff);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("iLink poll error: {e}");
                        sleep_with_abort(backoff, &abort).await;
                        backoff = next_backoff(backoff);
                    }
                }
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        tracing::info!("WeChat (iLink) adapter connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.poll_handle.take() {
            h.abort();
        }
        self.health = AdapterHealth::Disconnected;
        tracing::info!("WeChat (iLink) adapter disconnected");
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        let body = match &content {
            MessageContent::Text(text) => serde_json::json!({"chat_id": chat_id, "content": text}),
            MessageContent::Image { url, caption } => {
                let text = caption
                    .as_deref()
                    .map_or_else(|| url.clone(), |c| format!("{c} {url}"));
                serde_json::json!({"chat_id": chat_id, "content": text})
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "WeChat only supports text and image content"
                ));
            }
        };
        let resp = self
            .client
            .post(format!("{ILINK_BASE}/messages/send"))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await?;
        json["message_id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("iLink send failed: {json:?}"))
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
        &mut self.rx
    }
    fn health(&self) -> AdapterHealth {
        self.health.clone()
    }
}

/// Next capped-exponential backoff value (doubles, saturating at `BACKOFF_MAX`).
fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(BACKOFF_MAX)
}

/// Sleep for `dur`, but wake early if `abort` is set so shutdown stays prompt.
async fn sleep_with_abort(dur: Duration, abort: &AtomicBool) {
    let deadline = tokio::time::Instant::now() + dur;
    loop {
        if abort.load(Ordering::SeqCst) {
            return;
        }
        let step = Duration::from_millis(200).min(deadline - tokio::time::Instant::now());
        if step.is_zero() {
            return;
        }
        tokio::time::sleep(step).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ilink_message() {
        let json = serde_json::json!({
            "chat_id": "wx-chat-123",
            "sender_id": "user-456",
            "sender_name": "Test User",
            "content": "Hello from WeChat",
            "message_id": "msg-001"
        });
        let bm = WechatAdapter::parse_ilink_message(&json).unwrap();
        assert_eq!(bm.platform, Platform::Wechat);
        assert_eq!(bm.chat_id, "wx-chat-123");
        assert_eq!(bm.sender_id, "wx-user-456");
        assert_eq!(bm.sender_name, "Test User");
        assert!(bm.untrusted, "inbound wechat message must be untrusted");
    }

    #[test]
    fn test_parse_ilink_missing_fields() {
        let json = serde_json::json!({"content": "no chat_id"});
        assert!(WechatAdapter::parse_ilink_message(&json).is_none());
    }

    #[test]
    fn test_inbound_content_is_capped() {
        let big = "x".repeat(MAX_INBOUND_CONTENT_LEN + 50);
        let json = serde_json::json!({
            "chat_id": "c",
            "sender_id": "s",
            "content": big,
            "message_id": "m",
        });
        let bm = WechatAdapter::parse_ilink_message(&json).unwrap();
        match bm.content {
            MessageContent::Text(t) => assert!(t.ends_with(TRUNCATION_MARKER)),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn api_key_is_redacted() {
        let k = ApiKey::new("super-secret".to_string());
        assert_eq!(format!("{k}"), "***");
        assert_eq!(format!("{k:?}"), "ApiKey(***)");
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = WechatAdapter::new("test-key".into());
        assert_eq!(adapter.platform(), Platform::Wechat);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
