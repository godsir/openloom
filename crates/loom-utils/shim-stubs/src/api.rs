// Stub for codex-api types.

use std::collections::HashMap;
use std::sync::Arc;

/// Stub auth provider trait.
pub trait AuthProvider: Send + Sync {
    fn to_auth_headers(&self) -> http::HeaderMap {
        http::HeaderMap::new()
    }
    fn clone_auth_provider(&self) -> Box<dyn AuthProvider>;
}

/// Shared auth provider type alias.
pub type SharedAuthProvider = Arc<dyn AuthProvider>;
