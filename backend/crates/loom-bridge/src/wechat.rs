//! WechatAdapter — polls iLink API for WeChat messages.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::*;

const ILINK_BASE: &str = "https://api.ilink.ai/api/v1";

pub struct WechatAdapter {
    api_key: String,
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
            api_key,
            client: reqwest::Client::new(),
            health: AdapterHealth::Disconnected,
            rx,
            tx,
            abort: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }

    fn parse_ilink_message(msg: &serde_json::Value) -> Option<BridgeMessage> {
        Some(BridgeMessage {
            platform: Platform::Wechat,
            chat_id: msg.get("chat_id")?.as_str()?.to_string(),
            sender_id: format!("wx-{}", msg.get("sender_id")?.as_str()?),
            sender_name: msg
                .get("sender_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            content: MessageContent::Text(msg.get("content")?.as_str()?.to_string()),
            reply_to: None,
            external_message_id: msg.get("message_id")?.as_str()?.to_string(),
            timestamp: chrono::Utc::now(),
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
        self.abort.store(false, Ordering::SeqCst);
        let api_key = self.api_key.clone();
        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();

        let handle = tokio::spawn(async move {
            let mut since_id = String::new();
            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }
                let mut url = format!("{}/messages/poll", ILINK_BASE);
                if !since_id.is_empty() {
                    url.push_str(&format!("?since_id={}", since_id));
                }
                match client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await
                            && let Some(messages) = body["messages"].as_array()
                        {
                            for msg in messages {
                                since_id =
                                    msg["message_id"].as_str().unwrap_or(&since_id).to_string();
                                if let Some(bm) = Self::parse_ilink_message(msg) {
                                    let _ = tx.send(bm).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error=%e, "iLink poll error");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
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
                    .map_or_else(|| url.clone(), |c| format!("{} {}", c, url));
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
            .post(format!("{}/messages/send", ILINK_BASE))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await?;
        json["message_id"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("iLink send failed: {:?}", json))
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
        &mut self.rx
    }
    fn health(&self) -> AdapterHealth {
        self.health.clone()
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
    }

    #[test]
    fn test_parse_ilink_missing_fields() {
        let json = serde_json::json!({"content": "no chat_id"});
        assert!(WechatAdapter::parse_ilink_message(&json).is_none());
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = WechatAdapter::new("test-key".into());
        assert_eq!(adapter.platform(), Platform::Wechat);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
