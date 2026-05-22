//! OSS provider utilities shared between TUI and exec.

use loom_tui_stubs::config::Config;
use loom_tui_stubs::model_provider_info::LMSTUDIO_OSS_PROVIDER_ID;
use loom_tui_stubs::model_provider_info::OLLAMA_OSS_PROVIDER_ID;

/// Returns the default model for a given OSS provider.
pub fn get_default_model_for_oss_provider(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        LMSTUDIO_OSS_PROVIDER_ID => Some(loom_shim_stubs::DEFAULT_OSS_MODEL),
        OLLAMA_OSS_PROVIDER_ID => Some(loom_shim_stubs::DEFAULT_OSS_MODEL),
        _ => None,
    }
}

/// Ensures the specified OSS provider is ready (models downloaded, service reachable).
pub async fn ensure_oss_provider_ready(
    provider_id: &str,
    config: &Config,
) -> Result<(), std::io::Error> {
    match provider_id {
        LMSTUDIO_OSS_PROVIDER_ID => {
            loom_shim_stubs::ensure_oss_ready(config as &dyn std::any::Any)
                .await
                .map_err(|e| std::io::Error::other(format!("OSS setup failed: {e}")))?;
        }
        OLLAMA_OSS_PROVIDER_ID => {
            loom_shim_stubs::ensure_responses_supported(&config.model_provider as &dyn std::any::Any)
                .await
                .map_err(|e| std::io::Error::other(format!("responses check failed: {e}")))?;
            loom_shim_stubs::ensure_oss_ready(config as &dyn std::any::Any)
                .await
                .map_err(|e| std::io::Error::other(format!("OSS setup failed: {e}")))?;
        }
        _ => {
            // Unknown provider, skip setup
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_default_model_for_provider_lmstudio() {
        let result = get_default_model_for_oss_provider(LMSTUDIO_OSS_PROVIDER_ID);
        assert_eq!(result, Some(loom_shim_stubs::DEFAULT_OSS_MODEL));
    }

    #[test]
    fn test_get_default_model_for_provider_ollama() {
        let result = get_default_model_for_oss_provider(OLLAMA_OSS_PROVIDER_ID);
        assert_eq!(result, Some(loom_shim_stubs::DEFAULT_OSS_MODEL));
    }

    #[test]
    fn test_get_default_model_for_provider_unknown() {
        let result = get_default_model_for_oss_provider("unknown-provider");
        assert_eq!(result, None);
    }
}
