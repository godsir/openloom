use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::adapter::ChannelAdapter;
use super::types::*;

pub struct QQAdapter {
    app_id: String,
    token: String,
    client: Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    #[allow(dead_code)]
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    ws_handle: Option<JoinHandle<()>>,
    access_token: Option<String>,
}

impl QQAdapter {
    pub fn new(app_id: String, token: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            app_id, token,
            client: Client::new(),
            health: AdapterHealth::Disconnected,
            rx, tx,
            abort: Arc::new(AtomicBool::new(false)),
            ws_handle: None,
            access_token: None,
        }
    }

    async fn refresh_access_token(&mut self) -> Result<String> {
        let resp = self.client
            .post("https://bots.qq.com/app/getAppAccessToken")
            .json(&serde_json::json!({
                "appId": self.app_id,
                "clientSecret": self.token,
            }))
            .send().await?;
        let body: serde_json::Value = resp.json().await?;
        let token = body["access_token"].as_str()
            .ok_or_else(|| anyhow::anyhow!("missing access_token"))?
            .to_string();
        self.access_token = Some(token.clone());
        Ok(token)
    }

    // TODO: Wire into connect() WebSocket event loop when fully implemented
    #[allow(dead_code)]
    fn parse_c2c_message(event: &serde_json::Value) -> Option<BridgeMessage> {
        let data = event.get("data")?;
        let author = data.get("author")?;
        let sender_id = author.get("user_openid")?.as_str()?.to_string();
        let content = data.get("content")?.as_str()?.to_string();
        let message_id = data.get("id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::QQ,
            chat_id: sender_id.clone(),
            sender_id,
            sender_name: "QQ User".to_string(),
            content: MessageContent::Text(content),
            reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
        })
    }

    // TODO: Wire into connect() WebSocket event loop when fully implemented
    #[allow(dead_code)]
    fn parse_group_message(event: &serde_json::Value) -> Option<BridgeMessage> {
        let data = event.get("data")?;
        let group_id = data.get("group_openid")?.as_str()?.to_string();
        let author = data.get("author")?;
        let sender_id = author.get("member_openid")?.as_str()?.to_string();
        let content = data.get("content")?.as_str()?.to_string();
        let message_id = data.get("id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::QQ,
            chat_id: group_id,
            sender_id,
            sender_name: "QQ User".to_string(),
            content: MessageContent::Text(content),
            reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
        })
    }
}

#[async_trait]
impl ChannelAdapter for QQAdapter {
    fn platform(&self) -> Platform { Platform::QQ }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        let _token = self.refresh_access_token().await?;
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
        let token = self.access_token.as_ref()
            .ok_or_else(|| anyhow::anyhow!("no access token"))?;
        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            _ => anyhow::bail!("unsupported content type for QQ"),
        };

        let resp = self.client
            .post(&format!("https://api.sgroup.qq.com/v2/users/{chat_id}/messages"))
            .header("Authorization", format!("QQBot {token}"))
            .json(&serde_json::json!({
                "content": text,
                "msg_type": 0,
            }))
            .send().await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(body["id"].as_str().unwrap_or("").to_string())
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> { &mut self.rx }
    fn health(&self) -> AdapterHealth { self.health.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_c2c_message() {
        let event = serde_json::json!({
            "data": {
                "id": "msg_qq_1",
                "content": "你好机器人",
                "author": {"user_openid": "openid_123"}
            }
        });
        let msg = QQAdapter::parse_c2c_message(&event).unwrap();
        assert_eq!(msg.platform, Platform::QQ);
        assert_eq!(msg.sender_id, "openid_123");
        assert_eq!(msg.chat_id, "openid_123");
        match msg.content {
            MessageContent::Text(t) => assert_eq!(t, "你好机器人"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_parse_group_message() {
        let event = serde_json::json!({
            "data": {
                "id": "msg_qq_2",
                "content": "群里好",
                "group_openid": "group_456",
                "author": {"member_openid": "member_789"}
            }
        });
        let msg = QQAdapter::parse_group_message(&event).unwrap();
        assert_eq!(msg.chat_id, "group_456");
        assert_eq!(msg.sender_id, "member_789");
    }

    #[test]
    fn test_parse_c2c_missing_fields() {
        let event = serde_json::json!({"data": {"content": "hi"}});
        assert!(QQAdapter::parse_c2c_message(&event).is_none());
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = QQAdapter::new("id".into(), "token".into());
        assert_eq!(adapter.platform(), Platform::QQ);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
