use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::adapter::ChannelAdapter;
use super::types::*;

const ILINK_BASE: &str = "https://api.ilink.ai/api/v1";

pub struct WechatAdapter {
    api_key: String,
    client: Client,
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
            client: Client::new(),
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
        let chat_id = msg.get("chat_id")?.as_str()?.to_string();
        let sender_id = msg.get("sender_id")?.as_str()?.to_string();
        let sender_name = msg
            .get("sender_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let text = msg.get("content")?.as_str()?.to_string();
        let message_id = msg.get("message_id")?.as_str()?.to_string();

        Some(BridgeMessage {
            platform: Platform::Wechat,
            chat_id,
            sender_id,
            sender_name,
            content: MessageContent::Text(text),
            reply_to: None,
            external_message_id: message_id,
            timestamp: Utc::now(),
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

        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();
        let auth = self.auth_header();

        let handle = tokio::spawn(async move {
            let mut since_id: Option<String> = None;
            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }

                let mut url = format!("{ILINK_BASE}/messages/poll");
                if let Some(ref sid) = since_id {
                    url.push_str(&format!("?since_id={sid}"));
                }

                match client.get(&url).header("Authorization", &auth).send().await {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            if let Some(messages) = body.get("messages").and_then(|m| m.as_array())
                            {
                                for msg in messages {
                                    if let Some(id) = msg.get("message_id").and_then(|v| v.as_str())
                                    {
                                        since_id = Some(id.to_string());
                                    }
                                    if let Some(bridge_msg) = Self::parse_ilink_message(msg) {
                                        if tx.send(bridge_msg).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "ilink polling error");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.poll_handle.take() {
            h.abort();
        }
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            MessageContent::Image { url, caption } => {
                format!("{} {}", url, caption.as_deref().unwrap_or(""))
            }
            _ => anyhow::bail!("unsupported content type for wechat"),
        };

        let resp = self
            .client
            .post(&format!("{ILINK_BASE}/messages/send"))
            .header("Authorization", self.auth_header())
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "content": text,
            }))
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        Ok(body["message_id"].as_str().unwrap_or("").to_string())
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
        let msg = serde_json::json!({
            "message_id": "msg_001",
            "chat_id": "wx_chat_123",
            "sender_id": "wx_user_456",
            "sender_name": "张三",
            "content": "你好"
        });
        let bridge_msg = WechatAdapter::parse_ilink_message(&msg).unwrap();
        assert_eq!(bridge_msg.platform, Platform::Wechat);
        assert_eq!(bridge_msg.chat_id, "wx_chat_123");
        assert_eq!(bridge_msg.sender_name, "张三");
        match bridge_msg.content {
            MessageContent::Text(t) => assert_eq!(t, "你好"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_parse_ilink_missing_fields() {
        let msg = serde_json::json!({"content": "hi"});
        assert!(WechatAdapter::parse_ilink_message(&msg).is_none());
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = WechatAdapter::new("key".into());
        assert_eq!(adapter.platform(), Platform::Wechat);
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }

    #[test]
    fn test_auth_header() {
        let adapter = WechatAdapter::new("my_key_123".into());
        assert_eq!(adapter.auth_header(), "Bearer my_key_123");
    }
}
