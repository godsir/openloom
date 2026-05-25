use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::adapter::ChannelAdapter;
use super::types::*;

pub struct FeishuAdapter {
    app_id: String,
    app_secret: String,
    client: Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    #[allow(dead_code)]
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    ws_handle: Option<JoinHandle<()>>,
    tenant_access_token: Option<String>,
}

impl FeishuAdapter {
    pub fn new(app_id: String, app_secret: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            app_id, app_secret,
            client: Client::new(),
            health: AdapterHealth::Disconnected,
            rx, tx,
            abort: Arc::new(AtomicBool::new(false)),
            ws_handle: None,
            tenant_access_token: None,
        }
    }

    async fn refresh_token(&mut self) -> Result<String> {
        let resp = self.client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&serde_json::json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret,
            }))
            .send().await?;
        let body: serde_json::Value = resp.json().await?;
        let token = body["tenant_access_token"].as_str()
            .ok_or_else(|| anyhow::anyhow!("missing tenant_access_token"))?
            .to_string();
        self.tenant_access_token = Some(token.clone());
        Ok(token)
    }

    fn parse_event(event: &serde_json::Value) -> Option<BridgeMessage> {
        let msg = event.get("message")?;
        let chat_id = msg.get("chat_id")?.as_str()?.to_string();
        let sender = event.get("sender")?.get("sender_id")?.get("open_id")?.as_str()?.to_string();
        let sender_name = event.get("sender")
            .and_then(|s| s.get("sender_id"))
            .and_then(|s| s.get("union_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let msg_type = msg.get("message_type")?.as_str()?;
        let content_str = msg.get("content")?.as_str()?;

        let content = match msg_type {
            "text" => {
                let parsed: serde_json::Value = serde_json::from_str(content_str).ok()?;
                MessageContent::Text(parsed.get("text")?.as_str()?.to_string())
            }
            "image" => {
                let parsed: serde_json::Value = serde_json::from_str(content_str).ok()?;
                MessageContent::Image {
                    url: parsed.get("image_key")?.as_str()?.to_string(),
                    caption: None,
                }
            }
            _ => return None,
        };

        let message_id = msg.get("message_id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::Feishu,
            chat_id, sender_id: sender, sender_name,
            content, reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
        })
    }
}

#[async_trait]
impl ChannelAdapter for FeishuAdapter {
    fn platform(&self) -> Platform { Platform::Feishu }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        let _token = self.refresh_token().await?;
        self.abort.store(false, Ordering::SeqCst);
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.ws_handle.take() { h.abort(); }
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        let token = self.tenant_access_token.as_ref()
            .ok_or_else(|| anyhow::anyhow!("no access token"))?;

        let (msg_type, msg_content) = match &content {
            MessageContent::Text(text) => {
                ("text".to_string(), serde_json::json!({"text": text}).to_string())
            }
            MessageContent::Image { url, .. } => {
                ("image".to_string(), serde_json::json!({"image_key": url}).to_string())
            }
            _ => anyhow::bail!("unsupported content type for feishu"),
        };

        let resp = self.client
            .post("https://open.feishu.cn/open-apis/im/v1/messages")
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({
                "receive_id": chat_id,
                "msg_type": msg_type,
                "content": msg_content,
            }))
            .send().await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(body["data"]["message_id"].as_str().unwrap_or("").to_string())
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> { &mut self.rx }
    fn health(&self) -> AdapterHealth { self.health.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_event() {
        let event = serde_json::json!({
            "sender": {"sender_id": {"open_id": "ou_123", "union_id": "Alice"}},
            "message": {
                "message_id": "msg_456",
                "chat_id": "oc_789",
                "message_type": "text",
                "content": "{\"text\":\"hello\"}"
            }
        });
        let msg = FeishuAdapter::parse_event(&event).unwrap();
        assert_eq!(msg.platform, Platform::Feishu);
        assert_eq!(msg.chat_id, "oc_789");
        assert_eq!(msg.sender_id, "ou_123");
        match msg.content {
            MessageContent::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_parse_image_event() {
        let event = serde_json::json!({
            "sender": {"sender_id": {"open_id": "ou_1", "union_id": "Bob"}},
            "message": {
                "message_id": "msg_2",
                "chat_id": "oc_3",
                "message_type": "image",
                "content": "{\"image_key\":\"img_key_123\"}"
            }
        });
        let msg = FeishuAdapter::parse_event(&event).unwrap();
        match msg.content {
            MessageContent::Image { url, .. } => assert_eq!(url, "img_key_123"),
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn test_parse_unsupported_event_returns_none() {
        let event = serde_json::json!({
            "sender": {"sender_id": {"open_id": "ou_1"}},
            "message": {
                "message_id": "msg_1",
                "chat_id": "oc_1",
                "message_type": "sticker",
                "content": "{}"
            }
        });
        assert!(FeishuAdapter::parse_event(&event).is_none());
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = FeishuAdapter::new("id".into(), "secret".into());
        assert_eq!(adapter.platform(), Platform::Feishu);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
