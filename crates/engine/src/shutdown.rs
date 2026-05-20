use super::Engine;
use anyhow::Result;
use std::sync::atomic::Ordering;

impl Engine {
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("engine shutting down");
        self.draining.store(true, Ordering::SeqCst);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.in_flight.load(Ordering::SeqCst) > 0 {
            if tokio::time::Instant::now() > deadline {
                tracing::warn!(
                    "shutdown timeout, {} requests still in-flight",
                    self.in_flight.load(Ordering::SeqCst)
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if let Ok(conn) = rusqlite::Connection::open(&self.db_path) {
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        tracing::info!("engine shutdown complete");
        Ok(())
    }
}
