// Stub for codex-model-provider types.

use http::HeaderMap;
use std::sync::Arc;

/// Stub auth provider that returns empty headers.
#[derive(Clone)]
pub struct AuthProvider;

impl crate::api::AuthProvider for AuthProvider {
    fn to_auth_headers(&self) -> http::HeaderMap {
        http::HeaderMap::new()
    }
    fn clone_auth_provider(&self) -> Box<dyn crate::api::AuthProvider> {
        Box::new(self.clone())
    }
}

/// Stub function: returns Arc-wrapped auth provider.
pub fn auth_provider_from_auth(
    _auth: &crate::login::CodexAuth,
) -> Arc<dyn crate::api::AuthProvider> {
    Arc::new(AuthProvider)
}
