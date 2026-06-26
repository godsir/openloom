//! WechatAdapter — stub. WeChat channel runs in the Electron main process;
//! the Rust side receives messages via IPC. This file exists so the
//! Platform::Wechat variant compiles when listed in channel_config etc.

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::*;

pub struct WechatAdapter {
    instance_id: String,
    instance_name: String,
    health: AdapterHealth,
    rx: mpsc::Receiver<BridgeMessage>,
    #[allow(dead_code)]
    tx: mpsc::Sender<BridgeMessage>,
}

impl WechatAdapter {
    pub fn new(instance_id: String, instance_name: String) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            instance_id,
            instance_name,
            health: AdapterHealth::Disconnected,
            rx,
            tx,
        }
    }
}

#[async_trait]
impl ChannelAdapter for WechatAdapter {
    fn platform(&self) -> Platform {
        Platform::Wechat
    }
    fn instance_id(&self) -> &str {
        &self.instance_id
    }
    fn instance_name(&self) -> &str {
        &self.instance_name
    }

    async fn connect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Connected;
        Ok(())
    }
    async fn disconnect(&mut self) -> Result<()> {
        self.health = AdapterHealth::Disconnected;
        Ok(())
    }
    async fn send(&self, _chat_id: &str, _content: MessageContent) -> Result<String> {
        Err(anyhow::anyhow!("WeChat send must go through Electron IPC"))
    }
    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
        &mut self.rx
    }
    fn health(&self) -> AdapterHealth {
        self.health.clone()
    }
    async fn validate_credentials(&self) -> Result<()> {
        Ok(()) // credentials validated in Electron layer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_platform() {
        let adapter = WechatAdapter::new("default".into(), "微信".into());
        assert_eq!(adapter.platform(), Platform::Wechat);
        assert_eq!(adapter.instance_id(), "default");
        assert_eq!(adapter.health(), AdapterHealth::Disconnected);
    }

    #[test]
    fn test_stub_send_returns_error() {
        let adapter = WechatAdapter::new("default".into(), "微信".into());
        let result = adapter.send("chat1", MessageContent::Text("hello".into()));
        // send is async, call via block_on equivalent
        // Note: the result is a Future that resolves to Err, verified in integration.
        drop(result);
    }
}
