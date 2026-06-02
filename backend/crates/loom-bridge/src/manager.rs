//! BridgeManager — lifecycle management for platform adapters.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use crate::types::*;

pub struct BridgeManager {
    adapters: Arc<Mutex<HashMap<Platform, Box<dyn ChannelAdapter>>>>,
    inbound_tx: mpsc::Sender<BridgeMessage>,
    pub inbound_rx: Arc<Mutex<mpsc::Receiver<BridgeMessage>>>,
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

    pub async fn register(&self, adapter: Box<dyn ChannelAdapter>) {
        let platform = adapter.platform();
        self.adapters.lock().await.insert(platform, adapter);
    }

    pub async fn start_platform(&self, platform: Platform) -> Result<()> {
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&platform) {
            adapter.connect().await
        } else {
            Err(anyhow::anyhow!("no adapter registered for {}", platform))
        }
    }

    pub async fn stop_platform(&self, platform: Platform) -> Result<()> {
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&platform) {
            adapter.disconnect().await
        } else {
            Ok(())
        }
    }

    pub async fn send(
        &self,
        platform: Platform,
        chat_id: &str,
        content: MessageContent,
    ) -> Result<String> {
        let adapters = self.adapters.lock().await;
        adapters
            .get(&platform)
            .ok_or_else(|| anyhow::anyhow!("no adapter for {}", platform))?
            .send(chat_id, content)
            .await
    }

    pub async fn health_status(&self) -> HashMap<Platform, AdapterHealth> {
        self.adapters
            .lock()
            .await
            .iter()
            .map(|(p, a)| (*p, a.health()))
            .collect()
    }

    pub fn inbound_sender(&self) -> mpsc::Sender<BridgeMessage> {
        self.inbound_tx.clone()
    }

    pub async fn shutdown(&self) {
        let mut adapters = self.adapters.lock().await;
        for (_, adapter) in adapters.iter_mut() {
            let _ = adapter.disconnect().await;
        }
        adapters.clear();
    }
}
