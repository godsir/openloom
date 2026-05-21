pub mod app;
pub mod commands;
pub mod input;
pub mod keymap;
pub mod render;
pub mod status;
pub mod streaming;
pub mod theme;

mod overlays;

use std::sync::Arc;

use openloom_engine::Engine;

pub async fn run(engine: Arc<Engine>) -> anyhow::Result<()> {
    let _ = engine;
    Ok(())
}
