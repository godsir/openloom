// loom-shim-stubs: Stub types replacing transitive cut crate dependencies.
//
// All types are minimal stubs sufficient for compilation.
// Real implementations will be added as those crates are ported.

pub mod analytics;
pub mod code_mode;
pub mod config;
pub mod login;
pub mod model_provider;
pub mod otel;
pub mod plugin;
pub mod utils_output_truncation;
pub mod utils_pty;
