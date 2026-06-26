//! FeishuAdapter — placeholder (to be implemented in a follow-up task).

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::*;

pub struct FeishuAdapter {
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    #[allow(dead_code)]
    tx: mpsc::Sender<BridgeMessage>,
}

impl FeishuAdapter {
    #[allow(dead_code)]
    pub fn new(_config: serde_json::Value) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            health: AdapterHealth::Disconnected,
            rx,
            tx,
        }
    }
}

#[async_trait]
impl ChannelAdapter for FeishuAdapter {
    fn platform(&self) -> Platform {
        Platform::Feishu
    }

    fn instance_id(&self) -> &str {
        "default"
    }

    fn instance_name(&self) -> &str {
        "Feishu"
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connecting;
        tracing::warn!("FeishuAdapter is a placeholder; not implemented yet");
        self.health = AdapterHealth::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }

    async fn send(&self, _chat_id: &str, _content: MessageContent) -> Result<String> {
        Err(anyhow::anyhow!("FeishuAdapter not implemented"))
    }

    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
        &mut self.rx
    }

    fn health(&self) -> AdapterHealth {
        self.health.clone()
    }

    async fn validate_credentials(&self) -> Result<()> {
        Err(anyhow::anyhow!("FeishuAdapter not implemented"))
    }
}
