#![allow(dead_code, unused_imports, unused_variables, unused_mut)]
//! CLI recovery for local state database startup failures (stub).
//! Full state DB recovery deferred until loom-engine DB layer is stable.

use std::path::PathBuf;

/// Stub type for state DB startup errors.
#[derive(Debug)]
pub struct LocalStateDbStartupError {
    state_db_path: PathBuf,
    detail: String,
}

impl LocalStateDbStartupError {
    pub fn new(state_db_path: PathBuf, detail: String) -> Self {
        Self { state_db_path, detail }
    }
    pub fn state_db_path(&self) -> &PathBuf { &self.state_db_path }
    pub fn detail(&self) -> &str { &self.detail }
}

impl std::fmt::Display for LocalStateDbStartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "state DB error: {}", self.detail)
    }
}

impl std::error::Error for LocalStateDbStartupError {}

pub(crate) fn startup_error(_err: &std::io::Error) -> Option<&LocalStateDbStartupError> {
    // Stub: never detect startup errors
    None
}

pub(crate) fn is_locked(_detail: &str) -> bool {
    false
}

pub(crate) fn confirm_repair(_startup_error: &LocalStateDbStartupError) -> std::io::Result<bool> {
    Ok(false)
}

pub(crate) async fn repair_files(
    _startup_error: &LocalStateDbStartupError,
) -> std::io::Result<Vec<PathBuf>> {
    Ok(Vec::new())
}

pub(crate) fn print_repair_backups(_backups: &[PathBuf]) {}

pub(crate) fn print_diagnostic_guidance(startup_error: &LocalStateDbStartupError) {
    eprintln!("Loom couldn't start because its local database appears to be damaged.");
    eprintln!("Technical details: {}", startup_error.state_db_path().display());
}

pub(crate) fn print_locked_guidance(startup_error: &LocalStateDbStartupError) {
    eprintln!("Loom couldn't start because another process is using its local data.");
    eprintln!("Technical details: {}", startup_error.state_db_path().display());
}
