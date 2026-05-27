use openloom_engine::Engine;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkProxyConfig {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub http_proxy: String,
    #[serde(default)]
    pub https_proxy: String,
    #[serde(default)]
    pub ws_proxy: String,
    #[serde(default)]
    pub wss_proxy: String,
    #[serde(default = "default_no_proxy")]
    pub no_proxy: String,
}

fn default_mode() -> String {
    "system".to_string()
}
fn default_no_proxy() -> String {
    "localhost, 127.0.0.1, ::1".to_string()
}

fn parse_proxy_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip protocol prefix if present, we reconstruct it below
    let host_port = trimmed
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("socks://")
        .trim_start_matches("socks5://");
    if host_port.is_empty() {
        return None;
    }
    // Build http:// URL for reqwest (it supports http, https, socks5)
    Some(format!("http://{}", host_port))
}

/// Build a reqwest proxy from the engine's network_proxy config.
/// Returns None if no proxy should be used (direct mode).
pub fn build_reqwest_proxy(engine: &Engine) -> Option<reqwest::Proxy> {
    let settings = engine.read_settings();
    let config = match settings
        .get("network_proxy")
        .and_then(|v| serde_json::from_value::<NetworkProxyConfig>(v.clone()).ok())
    {
        Some(c) => c,
        None => return None,
    };

    if config.mode == "direct" {
        return None;
    }

    // For "system" mode, reqwest respects HTTP_PROXY/HTTPS_PROXY env vars automatically.
    // We return None to let reqwest use its default behavior.
    if config.mode == "system" {
        return None;
    }

    // Manual mode: build proxy from config fields
    let no_proxy = config.no_proxy.clone();
    let no_proxy_for_closure = no_proxy.clone();

    let mut proxy = reqwest::Proxy::custom(move |url| {
        let scheme = url.scheme();

        // Check no_proxy exclusions
        if let Some(host) = url.host_str() {
            if is_no_proxy_match(host, &no_proxy_for_closure) {
                return None; // direct connection
            }
        }

        let proxy_url = if scheme == "https" || scheme == "wss" {
            let raw = if !config.wss_proxy.is_empty() {
                &config.wss_proxy
            } else if !config.https_proxy.is_empty() {
                &config.https_proxy
            } else if !config.http_proxy.is_empty() {
                &config.http_proxy
            } else {
                &config.ws_proxy
            };
            parse_proxy_url(raw)
        } else {
            let raw = if !config.http_proxy.is_empty() {
                &config.http_proxy
            } else if !config.ws_proxy.is_empty() {
                &config.ws_proxy
            } else {
                &config.https_proxy
            };
            parse_proxy_url(raw)
        };

        proxy_url
    });

    // Also set NO_PROXY so reqwest respects it
    if !no_proxy.is_empty() {
        proxy = proxy.no_proxy(reqwest::NoProxy::from_string(&no_proxy));
    }

    Some(proxy)
}

fn is_no_proxy_match(host: &str, no_proxy: &str) -> bool {
    if no_proxy.is_empty() {
        return false;
    }
    let host_lower = host.to_lowercase();
    for entry in no_proxy.split(',') {
        let entry = entry.trim().to_lowercase();
        if entry.is_empty() {
            continue;
        }
        if entry == host_lower {
            return true;
        }
        // Suffix match: .example.com matches foo.example.com
        if entry.starts_with('.') && host_lower.ends_with(&entry) {
            return true;
        }
        // Wildcard: *.example.com
        if entry.starts_with("*.") {
            let suffix = &entry[1..]; // .example.com
            if host_lower.ends_with(suffix) {
                return true;
            }
        }
    }
    false
}

/// Build a reqwest Client with proxy from engine config.
pub fn build_client(engine: &Engine) -> reqwest::Client {
    let mut builder = reqwest::Client::builder();
    if let Some(proxy) = build_reqwest_proxy(engine) {
        builder = builder.proxy(proxy);
    }
    builder.build().unwrap_or_default()
}
