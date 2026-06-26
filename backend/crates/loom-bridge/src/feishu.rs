//! FeishuAdapter — direct Feishu Open Platform HTTP API (no npm SDK)

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::bot_info::BotInfo;
use crate::types::*;

const FEISHU_BASE: &str = "https://open.feishu.cn/open-apis";
const LARK_BASE: &str = "https://open.larksuite.com/open-apis";

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const BACKOFF_BASE: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct AppSecret(String);

impl AppSecret {
    fn reveal(&self) -> &str { &self.0 }
}

impl std::fmt::Debug for AppSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppSecret(***)")
    }
}
impl std::fmt::Display for AppSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

// Response types
#[derive(Deserialize)]
struct TenantAccessTokenResp {
    code: i32,
    msg: Option<String>,
    tenant_access_token: Option<String>,
}

#[derive(Deserialize)]
struct MessageListResp {
    code: i32,
    #[allow(dead_code)]
    msg: Option<String>,
    data: Option<MessageListData>,
}

#[derive(Deserialize)]
struct MessageListData {
    items: Vec<FeishuMessageItem>,
    has_more: Option<bool>,
    page_token: Option<String>,
}

#[derive(Deserialize)]
struct FeishuMessageItem {
    message_id: String,
    #[allow(dead_code)]
    msg_type: String,
    content: Option<String>,
    chat_id: Option<String>,
    sender: Option<FeishuSender>,
    #[allow(dead_code)]
    create_time: Option<String>,
}

#[derive(Deserialize)]
struct FeishuSender {
    id: Option<FeishuSenderId>,
}

#[derive(Deserialize)]
struct FeishuSenderId {
    open_id: Option<String>,
    union_id: Option<String>,
    user_id: Option<String>,
}

#[derive(Deserialize)]
struct BotInfoResp {
    code: i32,
    msg: Option<String>,
    data: Option<BotInfoData>,
}

#[derive(Deserialize)]
struct BotInfoData {
    app_name: Option<String>,
}

#[derive(Deserialize)]
struct SendMessageResp {
    code: i32,
    msg: Option<String>,
    data: Option<SendMessageData>,
}

#[derive(Deserialize)]
struct SendMessageData {
    message_id: Option<String>,
}

pub struct FeishuAdapter {
    instance_id: String,
    instance_name: String,
    app_id: String,
    app_secret: AppSecret,
    domain: String,
    client: reqwest::Client,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    tx: mpsc::Sender<BridgeMessage>,
    abort: Arc<AtomicBool>,
    poll_handle: Option<JoinHandle<()>>,
}

impl FeishuAdapter {
    pub fn new(
        instance_id: String,
        instance_name: String,
        app_id: String,
        app_secret: String,
        domain: String,
    ) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            instance_id,
            instance_name,
            app_id,
            app_secret: AppSecret(app_secret),
            domain,
            client: reqwest::Client::new(),
            health: AdapterHealth::Disconnected,
            rx,
            tx,
            abort: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }

    fn base_url(&self) -> &str {
        match self.domain.as_str() {
            "lark" => LARK_BASE,
            _ => FEISHU_BASE,
        }
    }

    async fn get_tenant_access_token(&self) -> Result<String> {
        let url = format!("{}/auth/v3/tenant_access_token/internal", self.base_url());
        let resp = self.client
            .post(&url)
            .json(&serde_json::json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret.reveal(),
            }))
            .send()
            .await?;
        let body: TenantAccessTokenResp = resp.json().await?;
        if body.code != 0 {
            return Err(anyhow::anyhow!(
                "Feishu tenant_access_token failed (code {}): {}",
                body.code,
                body.msg.as_deref().unwrap_or("unknown")
            ));
        }
        body.tenant_access_token
            .ok_or_else(|| anyhow::anyhow!("Feishu: missing tenant_access_token in response"))
    }

    async fn list_messages_raw(
        client: &reqwest::Client,
        base_url: &str,
        token: &str,
        page_token: Option<&str>,
        page_size: Option<u32>,
    ) -> Result<MessageListResp, reqwest::Error> {
        let ps = page_size.unwrap_or(20);
        let mut url = format!("{base_url}/im/v1/messages?page_size={ps}");
        if let Some(pt) = page_token {
            url.push_str(&format!("&page_token={pt}"));
        }
        client.get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send().await?
            .json().await
    }

    fn parse_message(item: &FeishuMessageItem) -> Option<BridgeMessage> {
        let content_str = item.content.as_ref()?;
        // Feishu message content is a JSON string; extract "text" field
        let text = serde_json::from_str::<serde_json::Value>(content_str)
            .ok()
            .and_then(|v| v.get("text")?.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| content_str.clone());
        let text = cap_inbound_text(&text);

        let sender_id = item.sender.as_ref()
            .and_then(|s| s.id.as_ref())
            .and_then(|id| id.open_id.clone()
                .or_else(|| id.user_id.clone())
                .or_else(|| id.union_id.clone()))
            .unwrap_or_default();

        Some(BridgeMessage {
            platform: Platform::Feishu,
            chat_id: item.chat_id.clone().unwrap_or_default(),
            sender_id: format!("fs-{sender_id}"),
            sender_name: "Feishu User".to_string(),
            content: MessageContent::Text(text),
            reply_to: None,
            external_message_id: item.message_id.clone(),
            timestamp: chrono::Utc::now(),
            untrusted: true,
        })
    }

    async fn send_text(&self, token: &str, receive_id: &str, text: &str) -> Result<String> {
        let url = format!("{}/im/v1/messages?receive_id_type=chat_id", self.base_url());
        let content_json = serde_json::to_string(&serde_json::json!({"text": text})).unwrap_or_default();
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "text",
            "content": content_json,
        });
        let resp = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await?;
        let body: SendMessageResp = resp.json().await?;
        if body.code != 0 {
            return Err(anyhow::anyhow!(
                "Feishu send message failed (code {}): {}",
                body.code,
                body.msg.as_deref().unwrap_or("unknown")
            ));
        }
        Ok(body.data.and_then(|d| d.message_id).unwrap_or_default())
    }
}

#[async_trait]
impl ChannelAdapter for FeishuAdapter {
    fn platform(&self) -> Platform { Platform::Feishu }
    fn instance_id(&self) -> &str { &self.instance_id }
    fn instance_name(&self) -> &str { &self.instance_name }

    async fn validate_credentials(&self) -> Result<()> {
        let token = self.get_tenant_access_token().await?;
        let url = format!("{}/bot/v3/info", self.base_url());
        let resp = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await?;
        let body: BotInfoResp = resp.json().await?;
        if body.code != 0 {
            return Err(anyhow::anyhow!(
                "Feishu bot info failed: {}",
                body.msg.as_deref().unwrap_or("unknown")
            ));
        }
        let app_name = body.data.and_then(|d| d.app_name).unwrap_or_default();
        tracing::info!("Feishu ({}) credentials validated, bot: {app_name}", self.instance_id);
        Ok(())
    }

    async fn get_bot_info(&self) -> Option<BotInfo> {
        let token = self.get_tenant_access_token().await.ok()?;
        let url = format!("{}/bot/v3/info", self.base_url());
        let body: BotInfoResp = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send().await.ok()?
            .json().await.ok()?;
        Some(BotInfo {
            username: body.data.as_ref().and_then(|d| d.app_name.clone()),
            display_name: Some(self.instance_name.clone()),
        })
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;

        // Validate credentials before starting poll
        if let Err(e) = self.validate_credentials().await {
            let msg = e.to_string();
            self.health = AdapterHealth::Error(msg.clone());
            tracing::error!("Feishu ({}) connect failed: {msg}", self.instance_id);
            return Err(e);
        }

        self.abort.store(false, Ordering::SeqCst);
        let client = self.client.clone();
        let tx = self.tx.clone();
        let abort = self.abort.clone();
        let base_url = self.base_url().to_string();
        let app_id = self.app_id.clone();
        let app_secret = self.app_secret.clone();

        let handle = tokio::spawn(async move {
            let mut page_token: Option<String> = None;
            let mut backoff = BACKOFF_BASE;

            loop {
                if abort.load(Ordering::SeqCst) { break; }

                // Get a fresh token each poll cycle
                let resp = client
                    .post(format!("{base_url}/auth/v3/tenant_access_token/internal"))
                    .json(&serde_json::json!({
                        "app_id": app_id,
                        "app_secret": app_secret.reveal(),
                    }))
                    .send()
                    .await;

                let token = match resp {
                    Ok(r) => match r.json::<TenantAccessTokenResp>().await {
                        Ok(resp) if resp.code == 0 => {
                            resp.tenant_access_token.unwrap_or_default()
                        }
                        _ => {
                            tracing::warn!("Feishu token refresh failed, backing off");
                            sleep_with_abort(backoff, &abort).await;
                            backoff = next_backoff(backoff);
                            continue;
                        }
                    },
                    Err(_) => {
                        tracing::warn!("Feishu token request failed, backing off");
                        sleep_with_abort(backoff, &abort).await;
                        backoff = next_backoff(backoff);
                        continue;
                    }
                };

                match Self::list_messages_raw(&client, &base_url, &token, page_token.as_deref(), None).await {
                    Ok(resp) if resp.code == 0 => {
                        backoff = BACKOFF_BASE;
                        if let Some(data) = &resp.data {
                            for item in &data.items {
                                if let Some(bm) = Self::parse_message(item) {
                                    if tx.send(bm).await.is_err() {
                                        tracing::warn!("Feishu inbound channel closed; stopping poll");
                                        return;
                                    }
                                }
                            }
                            page_token = data.page_token.clone();
                            if !data.has_more.unwrap_or(false) {
                                // No more pages — wait before next poll
                                sleep_with_abort(POLL_INTERVAL, &abort).await;
                                page_token = None;
                            }
                        }
                    }
                    Ok(resp) => {
                        tracing::warn!("Feishu list messages code={}", resp.code);
                        sleep_with_abort(backoff, &abort).await;
                        backoff = next_backoff(backoff);
                    }
                    Err(e) => {
                        tracing::warn!("Feishu poll error: {e}");
                        sleep_with_abort(backoff, &abort).await;
                        backoff = next_backoff(backoff);
                    }
                }
            }
        });

        self.poll_handle = Some(handle);
        self.health = AdapterHealth::Connected;
        tracing::info!("Feishu ({}) adapter connected", self.instance_id);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.abort.store(true, Ordering::SeqCst);
        if let Some(h) = self.poll_handle.take() { h.abort(); }
        self.health = AdapterHealth::Disconnected;
        tracing::info!("Feishu ({}) adapter disconnected", self.instance_id);
        Ok(())
    }

    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String> {
        match &content {
            MessageContent::Text(text) => {
                let token = self.get_tenant_access_token().await?;
                self.send_text(&token, chat_id, text).await
            }
            _ => Err(anyhow::anyhow!("Feishu only supports text messages currently")),
        }
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> { &mut self.rx }
    fn health(&self) -> AdapterHealth { self.health.clone() }
}

fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(BACKOFF_MAX)
}

async fn sleep_with_abort(dur: Duration, abort: &AtomicBool) {
    let deadline = tokio::time::Instant::now() + dur;
    loop {
        if abort.load(Ordering::SeqCst) { return; }
        let step = Duration::from_millis(200).min(deadline - tokio::time::Instant::now());
        if step.is_zero() { return; }
        tokio::time::sleep(step).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_feishu_text_message() {
        let item = FeishuMessageItem {
            message_id: "msg-001".to_string(),
            msg_type: "text".to_string(),
            content: Some(r#"{"text":"Hello from Feishu"}"#.to_string()),
            chat_id: Some("oc_chat123".to_string()),
            sender: Some(FeishuSender {
                id: Some(FeishuSenderId {
                    open_id: Some("ou_user456".to_string()),
                    union_id: None,
                    user_id: None,
                }),
            }),
            create_time: None,
        };
        let bm = FeishuAdapter::parse_message(&item).unwrap();
        assert_eq!(bm.platform, Platform::Feishu);
        assert_eq!(bm.chat_id, "oc_chat123");
        assert_eq!(bm.sender_id, "fs-ou_user456");
        assert!(bm.untrusted);
        match bm.content {
            MessageContent::Text(t) => assert_eq!(t, "Hello from Feishu"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn test_adapter_platform() {
        let adapter = FeishuAdapter::new(
            "default".into(), "Feishu".into(),
            "app1".into(), "secret1".into(), "feishu".into(),
        );
        assert_eq!(adapter.platform(), Platform::Feishu);
        assert_eq!(adapter.instance_id(), "default");
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }

    #[test]
    fn test_base_url_lark_vs_feishu() {
        let fs = FeishuAdapter::new("d".into(), "n".into(), "a".into(), "s".into(), "feishu".into());
        assert_eq!(fs.base_url(), FEISHU_BASE);
        let lark = FeishuAdapter::new("d".into(), "n".into(), "a".into(), "s".into(), "lark".into());
        assert_eq!(lark.base_url(), LARK_BASE);
    }

    #[test]
    fn test_app_secret_redacted() {
        let s = AppSecret("super-secret".to_string());
        assert_eq!(format!("{s}"), "***");
        assert_eq!(format!("{s:?}"), "AppSecret(***)");
        assert_eq!(s.reveal(), "super-secret");
    }
}
