//! TelegramAdapter — long-polls the Bot API, converts updates to BridgeMessages.

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::types::*;

pub struct TelegramAdapter {
    bot_token: String,
    client: reqwest::Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    poll_handle: Option<JoinHandle<()>>,
}

impl TelegramAdapter {
    pub fn new(bot_token: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            bot_token,
            client: reqwest::Client::new(),
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
        let content = msg
            .text
            .as_deref()
            .or(msg.caption.as_deref())
            .map(|t| MessageContent::Text(t.to_string()))?;

        let reply_to = msg
            .reply_to_message
            .as_ref()
            .and_then(|m| m.message_id.map(|id| id.to_string()));
        let sender_id = msg
            .from
            .as_ref()
            .map(|u| u.id.to_string())
            .unwrap_or_default();
        let sender_name = msg
            .from
            .as_ref()
            .map(|u| u.first_name.clone())
            .unwrap_or_default();

        Some(BridgeMessage {
            platform: Platform::Telegram,
            chat_id: msg.chat.id.to_string(),
            sender_id: format!("tg-{}", sender_id),
            sender_name,
            content,
            reply_to,
            external_message_id: msg.message_id.map(|id| id.to_string()).unwrap_or_default(),
            timestamp: chrono::Utc::now(),
        })
    }
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        self.abort.store(false, Ordering::SeqCst);
        let token = self.bot_token.clone();
        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();

        let handle = tokio::spawn(async move {
            let mut offset: i64 = 0;
            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }
                let url = format!(
                    "https://api.telegram.org/bot{}/getUpdates?timeout=30&offset={}",
                    token, offset
                );
                match client.get(&url).send().await {
                    Ok(resp) => {
                        if let Ok(body) = resp.json::<TelegramResponse>().await {
                            for update in body.result {
                                offset = update.update_id + 1;
                                if let Some(bm) = Self::update_to_bridge_message(&update) {
                                    let _ = tx.send(bm).await;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                }
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        tracing::info!("Telegram adapter connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.poll_handle.take() {
            h.abort();
        }
        self.health = AdapterHealth::Disconnected;
        tracing::info!("Telegram adapter disconnected");
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        match &content {
            MessageContent::Text(text) => {
                let resp = self
                    .client
                    .get(self.api_url("sendMessage"))
                    .query(&[("chat_id", chat_id), ("text", text.as_str())])
                    .send()
                    .await?;
                let json: serde_json::Value = resp.json().await?;
                json["result"]["message_id"]
                    .as_i64()
                    .map(|id| id.to_string())
                    .ok_or_else(|| anyhow::anyhow!("Telegram sendMessage failed: {:?}", json))
            }
            _ => Err(anyhow::anyhow!(
                "Telegram only supports text content for now"
            )),
        }
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
        &mut self.rx
    }
    fn health(&self) -> AdapterHealth {
        self.health.clone()
    }
}

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    result: Vec<TelegramUpdate>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: Option<i64>,
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
