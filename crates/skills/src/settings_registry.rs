/// Metadata for a single configurable setting.
#[derive(Debug, Clone)]
pub struct SettingMeta {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub setting_type: SettingType,
    pub scope: SettingScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingType {
    Toggle,
    List(&'static [&'static str]),
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingScope {
    Global,
    Agent,
}

/// The static registry of settings that the update_settings tool can modify.
pub fn settings_registry() -> &'static [SettingMeta] {
    &[
        SettingMeta {
            key: "agent.max_iterations",
            label: "Agent max iterations",
            description: "Maximum tool-calling rounds per request.",
            setting_type: SettingType::Text,
            scope: SettingScope::Agent,
        },
        SettingMeta {
            key: "agent.timeout_secs",
            label: "Agent timeout (seconds)",
            description: "Maximum time for an agent loop before timing out.",
            setting_type: SettingType::Text,
            scope: SettingScope::Agent,
        },
        SettingMeta {
            key: "server.host",
            label: "Server host",
            description: "The bind address for the HTTP/WS server.",
            setting_type: SettingType::Text,
            scope: SettingScope::Global,
        },
        SettingMeta {
            key: "logging.level",
            label: "Log level",
            description: "Minimum log level for tracing output.",
            setting_type: SettingType::List(&["INFO", "DEBUG", "WARN", "ERROR", "TRACE"]),
            scope: SettingScope::Global,
        },
        SettingMeta {
            key: "cache.total_budget_mb",
            label: "KV cache budget (MB)",
            description: "Maximum memory budget for the key-value cache.",
            setting_type: SettingType::Text,
            scope: SettingScope::Global,
        },
        SettingMeta {
            key: "router.keyword_threshold",
            label: "Router keyword threshold",
            description: "Confidence threshold for keyword-based intent matching (0.0–1.0).",
            setting_type: SettingType::Text,
            scope: SettingScope::Global,
        },
        SettingMeta {
            key: "persona.top_n",
            label: "Persona top-N",
            description: "Number of top cognitions included in the persona summary.",
            setting_type: SettingType::Text,
            scope: SettingScope::Agent,
        },
        SettingMeta {
            key: "persona.recency_decay_days",
            label: "Persona recency decay (days)",
            description: "Number of days over which cognition recency decays.",
            setting_type: SettingType::Text,
            scope: SettingScope::Agent,
        },
    ]
}
