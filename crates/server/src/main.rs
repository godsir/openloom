//! Minimal headless server binary for Electron sidecar mode.
//! Does NOT depend on any TUI/CLI infrastructure to avoid stack overflow issues.

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut port: u16 = 0;

    let mut i = 1;
    while i < args.len() {
        if args[i].as_str() == "--port" {
            i += 1;
            if i < args.len() {
                port = args[i].parse().unwrap_or(0);
            }
        }
        i += 1;
    }

    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openLoom");

    let config = openloom_engine::EngineConfig {
        data_dir,
        threshold: 2,
        cloud_config: None,
        local_config: None,
        rate_limit_ms: 0,
        heartbeat_interval_secs: 300,
        heartbeat_idle_threshold_min: 5,
        model_override: None,
        project_scope: String::new(),
        skip_permissions: true,
    };

    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    eprintln!("[loom-server] creating engine...");
    let engine = openloom_engine::Engine::new(config)?;

    eprintln!("[loom-server] starting server...");
    let server = openloom_server::Server::new(engine, None);
    drop(_guard);

    rt.block_on(server.serve(port))?;
    Ok(())
}
