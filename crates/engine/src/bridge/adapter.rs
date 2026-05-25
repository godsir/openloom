use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::{AdapterHealth, BridgeMessage, MessageContent, Platform};

/// Core trait that every platform adapter must implement.
///
/// Lifecycle: `connect()` → `receive_rx()` polls for messages → `send()` replies.
/// The BridgeManager owns adapter instances and manages their lifecycle.
#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// Platform identifier
    fn platform(&self) -> Platform;

    /// Connect to the platform API (start polling / WebSocket / webhook listener)
    async fn connect(&mut self) -> Result<()>;

    /// Disconnect gracefully
    async fn disconnect(&mut self) -> Result<()>;

    /// Send a message to a chat. Returns the platform-assigned external_message_id.
    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String>;

    /// Receiver for inbound messages. The adapter pushes messages here after
    /// normalizing them from the platform-specific format.
    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage>;

    /// Current health status
    fn health(&self) -> AdapterHealth;
}
