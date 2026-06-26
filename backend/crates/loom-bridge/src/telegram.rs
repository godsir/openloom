//! TelegramAdapter — long-polls the Bot API, converts updates to BridgeMessages.

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::bot_info::BotInfo;
use crate::store::{NullOffsetStore, OffsetStore};
use crate::types::*;

/// Bot token wrapper whose `Debug`/`Display` never reveal the secret.
///
/// The Telegram Bot API requires the token in the URL *path*, so the raw value
/// must still be readable for request construction (via [`BotToken::reveal`]),
/// but it must never end up in logs. Both `Debug` and `Display` print `***`,
/// so accidentally logging the token (or any struct that contains it) is inert.
#[derive(Clone)]
pub struct BotToken(String);

impl BotToken {
    fn new(token: String) -> Self {
        Self(token)
    }

    /// Reveal the raw token. ONLY for building request URLs — never log this.
    fn reveal(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for BotToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BotToken(***)")
    }
}

impl std::fmt::Display for BotToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

/// Backoff bounds for the poll loop when the API errors or returns non-`ok`.
const BACKOFF_BASE: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Key under which the confirmed update offset is persisted in the OffsetStore.
const OFFSET_KEY: &str = "telegram:offset";

pub struct TelegramAdapter {
    instance_id: String,
    instance_name: String,
    bot_token: BotToken,
    client: reqwest::Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    poll_handle: Option<JoinHandle<()>>,
    offset_store: Arc<dyn OffsetStore>,
}

impl TelegramAdapter {
    pub fn new(instance_id: String, instance_name: String, bot_token: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            instance_id,
            instance_name,
            bot_token: BotToken::new(bot_token),
            client: reqwest::Client::new(),
            health: AdapterHealth::Disconnected,
            rx,
            tx,
            abort: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
            offset_store: Arc::new(NullOffsetStore),
        }
    }

    /// Like [`TelegramAdapter::new`], but wires a persistent [`OffsetStore`] so
    /// the confirmed update offset survives process restarts. The store is only
    /// touched between `.await` points in the poll loop, so it must be cheap and
    /// non-blocking.
    pub fn with_offset_store(
        instance_id: String,
        instance_name: String,
        bot_token: String,
        offset_store: Arc<dyn OffsetStore>,
    ) -> Self {
        let mut adapter = Self::new(instance_id, instance_name, bot_token);
        adapter.offset_store = offset_store;
        adapter
    }

    fn api_url(&self, method: &str) -> String {
        // SECURITY: contains the secret token in the path — never log this URL.
        format!(
            "https://api.telegram.org/bot{}/{}",
            self.bot_token.reveal(),
            method
        )
    }

    /// Validate the token by calling `getMe`. Returns the bot username on a
    /// successful `ok:true` response; errors on non-`ok` JSON or HTTP failure.
    async fn validate(&self) -> Result<String> {
        let resp = self.client.get(self.api_url("getMe")).send().await?;
        let status = resp.status();
        // Read the body regardless so we can surface Telegram's description,
        // but never include the URL/token in the error.
        let body: GetMeResponse = resp.json().await.map_err(|e| {
            anyhow::anyhow!("Telegram getMe: invalid JSON response (HTTP {status}): {e}")
        })?;
        if body.ok {
            let username = body
                .result
                .and_then(|u| u.username)
                .unwrap_or_else(|| "unknown".to_string());
            Ok(username)
        } else {
            Err(anyhow::anyhow!(
                "Telegram getMe rejected the token (HTTP {status}): {}",
                body.description.as_deref().unwrap_or("ok=false")
            ))
        }
    }

    fn update_to_bridge_message(update: &TelegramUpdate) -> Option<BridgeMessage> {
        let msg = update.message.as_ref()?;
        let raw = msg.text.as_deref().or(msg.caption.as_deref())?;
        // Untrusted, arbitrary-length text from a remote user: cap at ingest.
        let content = MessageContent::Text(cap_inbound_text(raw));

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
            sender_id: format!("tg-{sender_id}"),
            sender_name,
            content,
            reply_to,
            external_message_id: msg.message_id.map(|id| id.to_string()).unwrap_or_default(),
            timestamp: chrono::Utc::now(),
            // Inbound from an external Telegram user: always untrusted.
            untrusted: true,
        })
    }
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn platform(&self) -> Platform {
        Platform::Telegram
    }

    fn instance_id(&self) -> &str {
        &self.instance_id
    }

    fn instance_name(&self) -> &str {
        &self.instance_name
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;

        // Validate the token BEFORE transitioning to Connected. On failure,
        // record the error health and surface it — do not start polling.
        let bot_username = match self.validate().await {
            Ok(name) => name,
            Err(e) => {
                let msg = e.to_string();
                self.health = AdapterHealth::Error(msg.clone());
                tracing::error!("Telegram ({}) connect failed: {msg}", self.instance_id);
                return Err(e);
            }
        };

        self.abort.store(false, Ordering::SeqCst);
        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();
        let offset_store = self.offset_store.clone();
        // Build the base getUpdates URL once. SECURITY: holds the token — the
        // task below never logs `base_url`.
        let base_url = self.api_url("getUpdates");

        let handle = tokio::spawn(async move {
            // Restore the confirmed offset so updates already handled before a
            // restart are not reprocessed.
            let mut offset: i64 = offset_store
                .load_offset(OFFSET_KEY)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let mut backoff = BACKOFF_BASE;

            loop {
                if abort.load(Ordering::SeqCst) {
                    break;
                }
                // Telegram confirms updates up to `offset - 1` by passing the
                // next expected id; long-poll up to 30s.
                let req = client
                    .get(&base_url)
                    .query(&[("timeout", "30"), ("offset", &offset.to_string())]);

                match req.send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        // HTTP 4xx (incl. 401 Unauthorized) and 409 Conflict are
                        // not transient — back off instead of spinning.
                        if status.is_client_error() {
                            tracing::warn!(
                                "Telegram getUpdates HTTP {status}; backing off {:?}",
                                backoff
                            );
                            sleep_with_abort(backoff, &abort).await;
                            backoff = next_backoff(backoff);
                            continue;
                        }

                        match resp.json::<TelegramResponse>().await {
                            Ok(body) if body.ok => {
                                backoff = BACKOFF_BASE; // healthy response: reset
                                for update in body.result {
                                    if let Some(bm) = Self::update_to_bridge_message(&update) {
                                        // Durably enqueue BEFORE advancing the
                                        // confirmed offset. If the receiver is
                                        // gone, stop — nothing will consume more.
                                        if tx.send(bm).await.is_err() {
                                            tracing::warn!(
                                                "Telegram inbound channel closed; stopping poll"
                                            );
                                            return;
                                        }
                                    }
                                    // Only now is this update handled: advance &
                                    // persist the confirmed offset so a crash
                                    // after this point will not replay it.
                                    offset = update.update_id + 1;
                                    if let Err(e) =
                                        offset_store.store_offset(OFFSET_KEY, &offset.to_string())
                                    {
                                        tracing::warn!("Telegram offset persist failed: {e}");
                                    }
                                }
                            }
                            Ok(body) => {
                                tracing::warn!(
                                    "Telegram getUpdates ok=false: {}; backing off {:?}",
                                    body.description.as_deref().unwrap_or("unknown"),
                                    backoff
                                );
                                sleep_with_abort(backoff, &abort).await;
                                backoff = next_backoff(backoff);
                            }
                            Err(e) => {
                                // Decode failure: do NOT drop silently and do NOT
                                // advance the offset (so updates are retried).
                                tracing::warn!("Telegram getUpdates decode failed: {e}");
                                sleep_with_abort(backoff, &abort).await;
                                backoff = next_backoff(backoff);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Telegram getUpdates request error: {e}");
                        sleep_with_abort(backoff, &abort).await;
                        backoff = next_backoff(backoff);
                    }
                }
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        // Log the bot identity, never the token/URL.
        tracing::info!("Telegram ({}) adapter connected as @{bot_username}", self.instance_id);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.poll_handle.take() {
            h.abort();
        }
        self.health = AdapterHealth::Disconnected;
        tracing::info!("Telegram ({}) adapter disconnected", self.instance_id);
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        match &content {
            MessageContent::Text(text) => {
                // POST with the params in the request BODY (form-encoded), so the
                // message text never appears in the URL/query string or logs.
                let resp = self
                    .client
                    .post(self.api_url("sendMessage"))
                    .form(&[("chat_id", chat_id), ("text", text.as_str())])
                    .send()
                    .await?;
                let json: serde_json::Value = resp.json().await?;
                json["result"]["message_id"]
                    .as_i64()
                    .map(|id| id.to_string())
                    .ok_or_else(|| anyhow::anyhow!("Telegram sendMessage failed: {json:?}"))
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

    async fn validate_credentials(&self) -> Result<()> {
        let username = self.validate().await?;
        tracing::info!("Telegram ({}) bot validated: @{username}", self.instance_id);
        Ok(())
    }

    async fn get_bot_info(&self) -> Option<BotInfo> {
        self.validate().await.ok().map(|username| BotInfo {
            username: Some(username),
            display_name: Some(self.instance_name.clone()),
        })
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

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    ok: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    result: Vec<TelegramUpdate>,
}

#[derive(Debug, Deserialize)]
struct GetMeResponse {
    ok: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    result: Option<TelegramBotUser>,
}

#[derive(Debug, Deserialize)]
struct TelegramBotUser {
    #[serde(default)]
    username: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_token_is_redacted_in_debug_and_display() {
        let t = BotToken::new("123456:SECRET_DO_NOT_LOG".to_string());
        assert_eq!(format!("{t}"), "***");
        assert_eq!(format!("{t:?}"), "BotToken(***)");
        // Sanity: the secret is still retrievable for URL construction.
        assert_eq!(t.reveal(), "123456:SECRET_DO_NOT_LOG");
    }

    #[test]
    fn inbound_text_is_capped_and_marked() {
        let big = "a".repeat(MAX_INBOUND_CONTENT_LEN + 100);
        let update = TelegramUpdate {
            update_id: 1,
            message: Some(TelegramMessage {
                message_id: Some(10),
                chat: TelegramChat { id: 42 },
                from: Some(TelegramUser {
                    id: 7,
                    first_name: "Bob".to_string(),
                }),
                text: Some(big),
                caption: None,
                reply_to_message: None,
            }),
        };
        let bm = TelegramAdapter::update_to_bridge_message(&update).unwrap();
        assert!(bm.untrusted, "inbound telegram message must be untrusted");
        match bm.content {
            MessageContent::Text(t) => {
                assert!(t.ends_with(TRUNCATION_MARKER));
                assert!(t.len() <= MAX_INBOUND_CONTENT_LEN + TRUNCATION_MARKER.len());
            }
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn backoff_is_capped() {
        let mut b = BACKOFF_BASE;
        for _ in 0..20 {
            b = next_backoff(b);
        }
        assert_eq!(b, BACKOFF_MAX);
    }
}
