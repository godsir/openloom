use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use super::adapter::ChannelAdapter;
use super::types::*;
use anyhow::Result;

/// Manages the lifecycle of all platform adapters and provides
/// a unified interface for sending/receiving bridge messages.
pub struct BridgeManager {
    adapters: Arc<Mutex<HashMap<Platform, Box<dyn ChannelAdapter>>>>,
    inbound_tx: mpsc::Sender<BridgeMessage>,
    #[allow(dead_code)]
    inbound_rx: Arc<Mutex<mpsc::Receiver<BridgeMessage>>>,
}

impl BridgeManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
            inbound_rx: Arc::new(Mutex::new(rx)),
            inbound_tx: tx,
        }
    }

    /// Register a platform adapter (does not start it)
    pub async fn register(&self, adapter: Box<dyn ChannelAdapter>) {
        let platform = adapter.platform();
        let mut adapters = self.adapters.lock().await;
        adapters.insert(platform, adapter);
        tracing::info!(platform = %platform, "bridge adapter registered");
    }

    /// Start the adapter for the given platform
    pub async fn start_platform(&self, platform: Platform) -> Result<()> {
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&platform) {
            adapter.connect().await?;
            tracing::info!(platform = %platform, "bridge adapter started");
            Ok(())
        } else {
            anyhow::bail!("no adapter registered for platform: {platform}")
        }
    }

    /// Stop the adapter for the given platform
    pub async fn stop_platform(&self, platform: Platform) -> Result<()> {
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&platform) {
            adapter.disconnect().await?;
            tracing::info!(platform = %platform, "bridge adapter stopped");
            Ok(())
        } else {
            anyhow::bail!("no adapter registered for platform: {platform}")
        }
    }

    /// Send a message through the appropriate platform adapter
    pub async fn send(
        &self,
        platform: Platform,
        chat_id: &str,
        content: MessageContent,
    ) -> Result<String> {
        let adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get(&platform) {
            adapter.send(chat_id, content).await
        } else {
            anyhow::bail!("no adapter for platform: {platform}")
        }
    }

    /// Get health status for all registered adapters
    pub async fn health_status(&self) -> HashMap<Platform, AdapterHealth> {
        let adapters = self.adapters.lock().await;
        adapters
            .iter()
            .map(|(p, a)| (*p, a.health()))
            .collect()
    }

    /// Get a clone of the inbound message sender (for adapters to push messages)
    pub fn inbound_sender(&self) -> mpsc::Sender<BridgeMessage> {
        self.inbound_tx.clone()
    }

    /// Stop all adapters
    pub async fn shutdown(&self) {
        let mut adapters = self.adapters.lock().await;
        for (platform, adapter) in adapters.iter_mut() {
            if let Err(e) = adapter.disconnect().await {
                tracing::warn!(platform = %platform, error = %e, "error stopping adapter");
            }
        }
        adapters.clear();
        tracing::info!("bridge manager shut down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockAdapter {
        platform: Platform,
        health: AdapterHealth,
        rx: mpsc::Receiver<BridgeMessage>,
        #[allow(dead_code)]
        tx: mpsc::Sender<BridgeMessage>,
    }

    impl MockAdapter {
        fn new(platform: Platform) -> Self {
            let (tx, rx) = mpsc::channel(16);
            Self {
                platform,
                health: AdapterHealth::Disconnected,
                rx,
                tx,
            }
        }
    }

    #[async_trait]
    impl ChannelAdapter for MockAdapter {
        fn platform(&self) -> Platform {
            self.platform
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
            Ok("mock_msg_id".to_string())
        }

        fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage> {
            &mut self.rx
        }

        fn health(&self) -> AdapterHealth {
            self.health.clone()
        }
    }

    #[tokio::test]
    async fn test_register_and_health() {
        let manager = BridgeManager::new();
        let adapter = MockAdapter::new(Platform::Telegram);
        manager.register(Box::new(adapter)).await;

        let health = manager.health_status().await;
        assert_eq!(health.len(), 1);
        assert_eq!(health[&Platform::Telegram], AdapterHealth::Disconnected);
    }

    #[tokio::test]
    async fn test_start_changes_health() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;

        manager.start_platform(Platform::Telegram).await.unwrap();

        let health = manager.health_status().await;
        assert_eq!(health[&Platform::Telegram], AdapterHealth::Connected);
    }

    #[tokio::test]
    async fn test_stop_changes_health() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;
        manager.start_platform(Platform::Telegram).await.unwrap();
        manager.stop_platform(Platform::Telegram).await.unwrap();

        let health = manager.health_status().await;
        assert_eq!(health[&Platform::Telegram], AdapterHealth::Disconnected);
    }

    #[tokio::test]
    async fn test_send_routes_to_correct_platform() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;
        manager.start_platform(Platform::Telegram).await.unwrap();

        let result = manager
            .send(
                Platform::Telegram,
                "chat_123",
                MessageContent::Text("hello".into()),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "mock_msg_id");
    }

    #[tokio::test]
    async fn test_send_fails_for_unregistered_platform() {
        let manager = BridgeManager::new();
        let result = manager
            .send(Platform::QQ, "chat_1", MessageContent::Text("hi".into()))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shutdown_clears_adapters() {
        let manager = BridgeManager::new();
        manager
            .register(Box::new(MockAdapter::new(Platform::Telegram)))
            .await;
        manager.shutdown().await;

        let health = manager.health_status().await;
        assert!(health.is_empty());
    }
}
