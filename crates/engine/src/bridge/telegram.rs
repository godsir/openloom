use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::adapter::ChannelAdapter;
use super::types::*;

pub struct TelegramAdapter {
    bot_token: String,
    client: Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    #[allow(dead_code)]
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    poll_handle: Option<JoinHandle<()>>,
}

impl TelegramAdapter {
    pub fn new(bot_token: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            bot_token,
            client: Client::new(),
            health: AdapterHealth::Disconnected,
            rx,
            tx,
            abort: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }

    fn update_to_bridge_message(update: &TelegramUpdate) -> Option<BridgeMessage> {
        let msg = update.message.as_ref()?;
        let chat_id = msg.chat.id.to_string();
        let (sender_id, sender_name) = if let Some(from) = &msg.from {
            (from.id.to_string(), from.first_name.clone())
        } else {
            ("unknown".to_string(), "Unknown".to_string())
        };

        let content = if let Some(text) = &msg.text {
            MessageContent::Text(text.clone())
        } else if let Some(caption) = &msg.caption {
            MessageContent::Image {
                url: String::new(),
                caption: Some(caption.clone()),
            }
        } else {
            return None;
        };

        Some(BridgeMessage {
            platform: Platform::Telegram,
            chat_id,
            sender_id,
            sender_name,
            content,
            reply_to: msg
                .reply_to_message
                .as_ref()
                .map(|r| r.message_id.to_string()),
            external_message_id: msg.message_id.to_string(),
            timestamp: Utc::now(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    #[serde(default)]
    result: Vec<TelegramUpdate>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    #[serde(default)]
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    chat: TelegramChat,
    from: Option<TelegramUser>,
    text: Option<String>,
    caption: Option<String>,
    reply_to_message: Option<Box<TelegramMessage>>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct TelegramUser {
    id: i64,
    first_name: String,
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        self.abort.store(false, Ordering::SeqCst);

        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();
        let bot_token = self.bot_token.clone();

        let handle = tokio::spawn(async move {
            let mut offset: i64 = 0;
            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }

                let url = format!("https://api.telegram.org/bot{}/getUpdates", bot_token);
                let resp = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "offset": offset,
                        "timeout": 30,
                        "allowed_updates": ["message"],
                    }))
                    .send()
                    .await;

                match resp {
                    Ok(r) => {
                        if let Ok(body) = r.json::<TelegramResponse>().await {
                            for update in &body.result {
                                offset = update.update_id + 1;
                                if let Some(bridge_msg) = Self::update_to_bridge_message(update)
                                    && tx.send(bridge_msg).await.is_err()
                                {
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "telegram polling error");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(handle) = self.poll_handle.take() {
            handle.abort();
        }
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        match content {
            MessageContent::Text(text) => {
                let resp = self
                    .client
                    .post(self.api_url("sendMessage"))
                    .json(&serde_json::json!({
                        "chat_id": chat_id,
                        "text": text,
                        "parse_mode": "Markdown",
                    }))
                    .send()
                    .await?;

                let body: serde_json::Value = resp.json().await?;
                let msg_id = body["result"]["message_id"]
                    .as_i64()
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                Ok(msg_id)
            }
            MessageContent::Image { url, caption } => {
                let resp = self
                    .client
                    .post(self.api_url("sendPhoto"))
                    .json(&serde_json::json!({
                        "chat_id": chat_id,
                        "photo": url,
                        "caption": caption.unwrap_or_default(),
                    }))
                    .send()
                    .await?;

                let body: serde_json::Value = resp.json().await?;
                let msg_id = body["result"]["message_id"]
                    .as_i64()
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                Ok(msg_id)
            }
            _ => {
                anyhow::bail!(
                    "unsupported content type for telegram: {}",
                    content.media_type()
                )
            }
        }
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
    fn test_api_url() {
        let adapter = TelegramAdapter::new("123456:ABC".to_string());
        assert_eq!(
            adapter.api_url("getUpdates"),
            "https://api.telegram.org/bot123456:ABC/getUpdates"
        );
    }

    #[test]
    fn test_update_to_bridge_message_text() {
        let update = TelegramUpdate {
            update_id: 1,
            message: Some(TelegramMessage {
                message_id: 100,
                chat: TelegramChat { id: 12345 },
                from: Some(TelegramUser {
                    id: 67890,
                    first_name: "Alice".to_string(),
                }),
                text: Some("hello bot".to_string()),
                caption: None,
                reply_to_message: None,
            }),
        };

        let msg = TelegramAdapter::update_to_bridge_message(&update).unwrap();
        assert_eq!(msg.platform, Platform::Telegram);
        assert_eq!(msg.chat_id, "12345");
        assert_eq!(msg.sender_id, "67890");
        assert_eq!(msg.sender_name, "Alice");
        match &msg.content {
            MessageContent::Text(t) => assert_eq!(t, "hello bot"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_update_to_bridge_message_none_for_empty() {
        let update = TelegramUpdate {
            update_id: 1,
            message: Some(TelegramMessage {
                message_id: 100,
                chat: TelegramChat { id: 12345 },
                from: None,
                text: None,
                caption: None,
                reply_to_message: None,
            }),
        };

        assert!(TelegramAdapter::update_to_bridge_message(&update).is_none());
    }

    #[tokio::test]
    async fn test_adapter_lifecycle() {
        let mut adapter = TelegramAdapter::new("fake_token".to_string());
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
        assert_eq!(adapter.platform(), Platform::Telegram);

        adapter.disconnect().await.unwrap();
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }
}
