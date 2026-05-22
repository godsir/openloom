// Stub for codex-client types.

/// Stub: builds a reqwest client. Returns a default client.
pub fn build_reqwest_client_with_custom_ca(
    builder: reqwest::ClientBuilder,
) -> Result<reqwest::Client, reqwest::Error> {
    builder.build()
}
