// Stub for codex-model-provider types.

use http::HeaderMap;

/// Stub auth provider that returns empty headers.
pub struct AuthProvider;

impl AuthProvider {
    pub fn to_auth_headers(&self) -> HeaderMap {
        HeaderMap::new()
    }
}

/// Stub function: returns empty auth provider.
pub fn auth_provider_from_auth(
    _auth: &crate::login::CodexAuth,
) -> AuthProvider {
    AuthProvider
}
