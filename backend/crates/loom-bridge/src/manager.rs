//! BridgeManager — lifecycle management for platform adapters.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, mpsc};

use crate::channel_config::InstanceConfig;
use crate::types::*;

type AdapterKey = (Platform, String);

pub struct BridgeManager {
    adapters: Arc<Mutex<HashMap<AdapterKey, Box<dyn ChannelAdapter>>>>,
    configs: Arc<RwLock<Vec<InstanceConfig>>>,
    inbound_tx: mpsc::Sender<BridgeMessage>,
    pub inbound_rx: Arc<Mutex<mpsc::Receiver<BridgeMessage>>>,
}

impl BridgeManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
            configs: Arc::new(RwLock::new(Vec::new())),
            inbound_rx: Arc::new(Mutex::new(rx)),
            inbound_tx: tx,
        }
    }

    /// Register an adapter along with its config
    pub async fn register(&self, config: InstanceConfig, adapter: Box<dyn ChannelAdapter>) {
        let key = (config.platform, config.instance_id.clone());
        self.adapters.lock().await.insert(key, adapter);
        self.configs.write().await.push(config);
    }

    /// Start a specific instance
    pub async fn start_instance(&self, platform: Platform, instance_id: &str) -> Result<()> {
        let key = (platform, instance_id.to_string());
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&key) {
            adapter.connect().await
        } else {
            Err(anyhow::anyhow!("no adapter for {platform}:{instance_id}"))
        }
    }

    /// Stop a specific instance
    pub async fn stop_instance(&self, platform: Platform, instance_id: &str) -> Result<()> {
        let key = (platform, instance_id.to_string());
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&key) {
            adapter.disconnect().await
        } else {
            Ok(())
        }
    }

    /// Start all enabled instances
    pub async fn start_all_enabled(&self) {
        let configs = self.configs.read().await;
        for cfg in configs.iter().filter(|c| c.enabled) {
            let _ = self.start_instance(cfg.platform, &cfg.instance_id).await;
        }
    }

    /// Stop all instances
    pub async fn shutdown_all(&self) {
        let keys: Vec<AdapterKey> = { self.adapters.lock().await.keys().cloned().collect() };
        for key in keys {
            let _ = self.stop_instance(key.0, &key.1).await;
        }
    }

    /// Send message via a specific instance
    pub async fn send(
        &self,
        platform: Platform,
        instance_id: &str,
        chat_id: &str,
        content: MessageContent,
    ) -> Result<String> {
        let key = (platform, instance_id.to_string());
        let adapters = self.adapters.lock().await;
        adapters
            .get(&key)
            .ok_or_else(|| anyhow::anyhow!("no adapter for {platform}:{instance_id}"))?
            .send(chat_id, content)
            .await
    }

    /// Health status for all instances
    pub async fn health_status(&self) -> HashMap<String, AdapterHealth> {
        self.adapters
            .lock()
            .await
            .iter()
            .map(|((p, id), a)| (format!("{p}:{id}"), a.health()))
            .collect()
    }

    /// Health status filtered by platform
    pub async fn platform_health(&self, platform: Platform) -> Vec<(String, AdapterHealth)> {
        self.adapters
            .lock()
            .await
            .iter()
            .filter(|((p, _), _)| *p == platform)
            .map(|((_, id), a)| (id.clone(), a.health()))
            .collect()
    }

    /// List all configs
    pub async fn list_configs(&self) -> Vec<InstanceConfig> {
        self.configs.read().await.clone()
    }

    /// Upsert a config (memory only — caller persists to SQLite separately)
    pub async fn upsert_config(&self, config: InstanceConfig) {
        let mut configs = self.configs.write().await;
        if let Some(existing) = configs
            .iter_mut()
            .find(|c| c.platform == config.platform && c.instance_id == config.instance_id)
        {
            *existing = config;
        } else {
            configs.push(config);
        }
    }

    /// Remove a config
    pub async fn remove_config(&self, platform: Platform, instance_id: &str) {
        let mut configs = self.configs.write().await;
        configs.retain(|c| !(c.platform == platform && c.instance_id == instance_id));
    }

    /// Validate credentials for connectivity test
    pub async fn validate_credentials(&self, platform: Platform, instance_id: &str) -> Result<()> {
        let key = (platform, instance_id.to_string());
        let mut adapters = self.adapters.lock().await;
        if let Some(adapter) = adapters.get_mut(&key) {
            adapter.validate_credentials().await
        } else {
            Err(anyhow::anyhow!("no adapter for {platform}:{instance_id}"))
        }
    }

    pub fn inbound_sender(&self) -> mpsc::Sender<BridgeMessage> {
        self.inbound_tx.clone()
    }
}
