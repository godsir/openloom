#![allow(dead_code, unused_imports, unused_variables, unused_mut)]
use crate::history_cell::PlainHistoryCell;
use crate::legacy_core::config::Config;
use crate::session_state::SessionNetworkProxyRuntime;
use loom_app_server_protocol::ConfigLayerSource;
use loom_config::ConfigLayerEntry;
use loom_config::ManagedHooksRequirementsToml;
use loom_config::NetworkConstraints;
use loom_config::NetworkDomainPermissionToml;
use loom_config::NetworkUnixSocketPermissionToml;
use loom_config::RequirementSource;
use loom_config::ResidencyRequirement;
use loom_config::SandboxModeRequirement;
use loom_config::WebSearchModeRequirement;
use ratatui::style::Stylize;
use ratatui::text::Line;
use toml::Value as TomlValue;

pub(crate) fn new_debug_config_output(
    config: &Config,
    session_network_proxy: Option<&SessionNetworkProxyRuntime>,
) -> PlainHistoryCell {
    let mut lines = render_debug_config_lines(&config.config_layer_stack);

    if let Some(proxy) = session_network_proxy {
        lines.push("".into());
        lines.push("Session runtime:".bold().into());
        lines.push("  - network_proxy".into());
        let SessionNetworkProxyRuntime {
            http_addr,
            socks_addr,
        } = proxy;
        let all_proxy = session_all_proxy_url(
            http_addr,
            socks_addr,
            config
                .permissions
                .network
                .as_ref()
                .is_some_and(|_| crate::legacy_core::config::NetworkProxySpec::socks_enabled()),
        );
        lines.push(format!("    - HTTP_PROXY  = http://{http_addr}").into());
        lines.push(format!("    - ALL_PROXY   = {all_proxy}").into());
    }

    PlainHistoryCell::new(lines)
}

fn session_all_proxy_url(http_addr: &str, socks_addr: &str, socks_enabled: bool) -> String {
    if socks_enabled {
        format!("socks5h://{socks_addr}")
    } else {
        format!("http://{http_addr}")
    }
}

fn render_debug_config_lines(_stack: &loom_tui_stubs::config::ConfigLayerStackStub) -> Vec<Line<'static>> {
    let mut lines = vec!["/debug-config".magenta().into(), "".into()];
    lines.push("Config layer stack (debug info not available in stub build):".bold().into());
    lines.push("  <stub - requires full loom_config::ConfigLayerStack>".dim().into());
    lines
}

fn render_non_file_layer_details(layer: &ConfigLayerEntry) -> Vec<Line<'static>> {
    match &layer.name {
        ConfigLayerSource::SessionFlags => render_session_flag_details(&layer.config),
        ConfigLayerSource::Mdm { .. } | ConfigLayerSource::LegacyManagedConfigTomlFromMdm => {
            render_mdm_layer_details(layer)
        }
        ConfigLayerSource::System { .. }
        | ConfigLayerSource::User { .. }
        | ConfigLayerSource::Project { .. }
        | ConfigLayerSource::LegacyManagedConfigTomlFromFile { .. } => Vec::new(),
    }
}

fn render_session_flag_details(config: &TomlValue) -> Vec<Line<'static>> {
    let mut pairs = Vec::new();
    flatten_toml_key_values(config, /*prefix*/ None, &mut pairs);

    if pairs.is_empty() {
        return vec!["     - <none>".dim().into()];
    }

    pairs
        .into_iter()
        .map(|(key, value)| format!("     - {key} = {value}").into())
        .collect()
}

fn format_managed_hooks_requirements(hooks: &ManagedHooksRequirementsToml) -> String {
    let mut parts = Vec::new();

    if let Some(managed_dir) = hooks.managed_dir.as_ref() {
        parts.push(format!("managed_dir={}", managed_dir.display()));
    }
    if let Some(windows_managed_dir) = hooks.windows_managed_dir.as_ref() {
        parts.push(format!(
            "windows_managed_dir={}",
            windows_managed_dir.display()
        ));
    }
    parts.push(format!("handlers={}", hooks.handler_count()));

    join_or_empty(parts)
}

fn render_mdm_layer_details(layer: &ConfigLayerEntry) -> Vec<Line<'static>> {
    let value = layer
        .raw_toml()
        .map(ToString::to_string)
        .unwrap_or_else(|| format_toml_value(&layer.config));
    if value.is_empty() {
        return vec!["     MDM value: <empty>".dim().into()];
    }

    if value.contains('\n') {
        let mut lines = vec!["     MDM value:".into()];
        lines.extend(value.lines().map(|line| format!("       {line}").into()));
        lines
    } else {
        vec![format!("     MDM value: {value}").into()]
    }
}

fn flatten_toml_key_values(
    value: &TomlValue,
    prefix: Option<&str>,
    out: &mut Vec<(String, String)>,
) {
    match value {
        TomlValue::Table(table) => {
            let mut entries = table.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(key, _)| key.as_str());
            for (key, child) in entries {
                let next_prefix = if let Some(prefix) = prefix {
                    format!("{prefix}.{key}")
                } else {
                    key.to_string()
                };
                flatten_toml_key_values(child, Some(&next_prefix), out);
            }
        }
        _ => {
            let key = prefix.unwrap_or("<value>").to_string();
            out.push((key, format_toml_value(value)));
        }
    }
}

fn format_toml_value(value: &TomlValue) -> String {
    value.to_string()
}

fn requirement_line(
    name: &str,
    value: String,
    source: Option<&RequirementSource>,
) -> Line<'static> {
    let source = source
        .map(ToString::to_string)
        .unwrap_or_else(|| "<unspecified>".to_string());
    format!("  - {name}: {value} (source: {source})").into()
}

fn join_or_empty(values: Vec<String>) -> String {
    if values.is_empty() {
        "<empty>".to_string()
    } else {
        values.join(", ")
    }
}

fn normalize_allowed_web_search_modes(
    modes: &[WebSearchModeRequirement],
) -> Vec<WebSearchModeRequirement> {
    if modes.is_empty() {
        return vec![WebSearchModeRequirement::Disabled];
    }

    let mut normalized = modes.to_vec();
    if !normalized.contains(&WebSearchModeRequirement::Disabled) {
        normalized.push(WebSearchModeRequirement::Disabled);
    }
    normalized
}

fn format_config_layer_source(source: &ConfigLayerSource) -> String {
    match source {
        ConfigLayerSource::Mdm { domain, key } => {
            format!("MDM ({domain}:{key})")
        }
        ConfigLayerSource::System { file } => {
            format!("system ({})", file.as_path().display())
        }
        ConfigLayerSource::User { file, .. } => {
            format!("user ({})", file.as_path().display())
        }
        ConfigLayerSource::Project { dot_codex_folder } => {
            format!(
                "project ({}/config.toml)",
                dot_codex_folder.as_path().display()
            )
        }
        ConfigLayerSource::SessionFlags => "session-flags".to_string(),
        ConfigLayerSource::LegacyManagedConfigTomlFromFile { file } => {
            format!("legacy managed_config.toml ({})", file.as_path().display())
        }
        ConfigLayerSource::LegacyManagedConfigTomlFromMdm => {
            "legacy managed_config.toml (MDM)".to_string()
        }
    }
}

fn format_sandbox_mode_requirement(mode: SandboxModeRequirement) -> String {
    match mode {
        SandboxModeRequirement::ReadOnly => "read-only".to_string(),
        SandboxModeRequirement::WorkspaceWrite => "workspace-write".to_string(),
        SandboxModeRequirement::DangerFullAccess => "danger-full-access".to_string(),
        SandboxModeRequirement::ExternalSandbox => "external-sandbox".to_string(),
    }
}

fn format_residency_requirement(requirement: ResidencyRequirement) -> String {
    match requirement {
        ResidencyRequirement::Us => "us".to_string(),
    }
}

fn format_network_constraints(network: &NetworkConstraints) -> String {
    let mut parts = Vec::new();

    let NetworkConstraints {
        enabled,
        http_port,
        socks_port,
        allow_upstream_proxy,
        dangerously_allow_non_loopback_proxy,
        dangerously_allow_all_unix_sockets,
        domains,
        managed_allowed_domains_only,
        unix_sockets,
        allow_local_binding,
    } = network;

    if let Some(enabled) = enabled {
        parts.push(format!("enabled={enabled}"));
    }
    if let Some(http_port) = http_port {
        parts.push(format!("http_port={http_port}"));
    }
    if let Some(socks_port) = socks_port {
        parts.push(format!("socks_port={socks_port}"));
    }
    if let Some(allow_upstream_proxy) = allow_upstream_proxy {
        parts.push(format!("allow_upstream_proxy={allow_upstream_proxy}"));
    }
    if let Some(dangerously_allow_non_loopback_proxy) = dangerously_allow_non_loopback_proxy {
        parts.push(format!(
            "dangerously_allow_non_loopback_proxy={dangerously_allow_non_loopback_proxy}"
        ));
    }
    if let Some(dangerously_allow_all_unix_sockets) = dangerously_allow_all_unix_sockets {
        parts.push(format!(
            "dangerously_allow_all_unix_sockets={dangerously_allow_all_unix_sockets}"
        ));
    }
    if let Some(domains) = domains {
        parts.push(format!(
            "domains={}",
            format_network_permission_entries(&domains.entries, format_network_domain_permission)
        ));
    }
    if let Some(managed_allowed_domains_only) = managed_allowed_domains_only {
        parts.push(format!(
            "managed_allowed_domains_only={managed_allowed_domains_only}"
        ));
    }
    if let Some(unix_sockets) = unix_sockets {
        parts.push(format!(
            "unix_sockets={}",
            format_network_permission_entries(
                &unix_sockets.entries,
                format_network_unix_socket_permission,
            )
        ));
    }
    if let Some(allow_local_binding) = allow_local_binding {
        parts.push(format!("allow_local_binding={allow_local_binding}"));
    }

    join_or_empty(parts)
}

fn format_network_permission_entries<T: Copy>(
    entries: &std::collections::BTreeMap<String, T>,
    format_value: impl Fn(T) -> &'static str,
) -> String {
    let parts = entries
        .iter()
        .map(|(key, value)| format!("{key}={}", format_value(*value)))
        .collect::<Vec<_>>();
    format!("{{{}}}", parts.join(", "))
}

fn format_network_domain_permission(permission: NetworkDomainPermissionToml) -> &'static str {
    match permission {
        NetworkDomainPermissionToml::Allow => "allow",
        NetworkDomainPermissionToml::Deny => "deny",
    }
}

fn format_network_unix_socket_permission(
    permission: NetworkUnixSocketPermissionToml,
) -> &'static str {
    match permission {
        NetworkUnixSocketPermissionToml::Allow => "allow",
        NetworkUnixSocketPermissionToml::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::render_debug_config_lines;
    use super::session_all_proxy_url;
    use crate::legacy_core::config::Constrained;
    use loom_app_server_protocol::AskForApproval;
    use loom_app_server_protocol::ConfigLayerSource;
    use loom_config::ConfigLayerEntry;
    use loom_config::ConfigLayerStack;
    use loom_config::ConfigRequirements;
    use loom_config::ConfigRequirementsToml;
    use loom_config::ConstrainedWithSource;
    use loom_config::FeatureRequirementsToml;
    use loom_config::FilesystemConstraints;
    use loom_config::HookEventsToml;
    use loom_config::HookHandlerConfig;
    use loom_config::ManagedHooksRequirementsToml;
    use loom_config::MatcherGroup;
    use loom_config::McpServerIdentity;
    use loom_config::McpServerRequirement;
    use loom_config::NetworkConstraints;
    use loom_config::NetworkDomainPermissionToml;
    use loom_config::NetworkDomainPermissionsToml;
    use loom_config::NetworkUnixSocketPermissionToml;
    use loom_config::NetworkUnixSocketPermissionsToml;
    use loom_config::RequirementSource;
    use loom_config::ResidencyRequirement;
    use loom_config::SandboxModeRequirement;
    use loom_config::Sourced;
    use loom_config::WebSearchModeRequirement;
    use loom_protocol::config_types::ApprovalsReviewer;
    use loom_protocol::config_types::WebSearchMode;
    use loom_protocol::models::PermissionProfile;
    use loom_absolute_path::AbsolutePathBuf;
    use ratatui::text::Line;
    use std::collections::BTreeMap;
    use toml::Value as TomlValue;

    fn empty_toml_table() -> TomlValue {
        TomlValue::Table(toml::map::Map::new())
    }

    fn absolute_path(path: &str) -> AbsolutePathBuf {
        AbsolutePathBuf::from_absolute_path(path).expect("absolute path")
    }

    fn render_to_text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn debug_config_output_lists_all_layers_including_disabled() {
        let system_file = if cfg!(windows) {
            absolute_path("C:\\etc\\codex\\config.toml")
        } else {
            absolute_path("/etc/codex/config.toml")
        };
        let project_folder = if cfg!(windows) {
            absolute_path("C:\\repo\\.codex")
        } else {
            absolute_path("/repo/.codex")
        };

        let layers = vec![
            ConfigLayerEntry::new(
                ConfigLayerSource::System { file: system_file },
                empty_toml_table(),
            ),
            ConfigLayerEntry::new_disabled(
                ConfigLayerSource::Project {
                    dot_codex_folder: project_folder,
                },
                empty_toml_table(),
                "project is untrusted",
            ),
        ];
        let stack = ConfigLayerStack::new(
            layers,
            ConfigRequirements::default(),
            ConfigRequirementsToml::default(),
        )
        .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(rendered.contains("(enabled)"));
        assert!(rendered.contains("(disabled)"));
        assert!(rendered.contains("reason: project is untrusted"));
        assert!(rendered.contains("Requirements:"));
        assert!(rendered.contains("  <none>"));
    }

    #[test]
    fn debug_config_output_lists_requirement_sources() {
        let requirements_file = if cfg!(windows) {
            absolute_path("C:\\ProgramData\\OpenAI\\Codex\\requirements.toml")
        } else {
            absolute_path("/etc/codex/requirements.toml")
        };
        let denied_path = if cfg!(windows) {
            absolute_path("C:\\Users\\alice\\.gitconfig")
        } else {
            absolute_path("/home/alice/.gitconfig")
        };

        let requirements = ConfigRequirements {
            approval_policy: ConstrainedWithSource::new(
                Constrained::allow_any(AskForApproval::OnRequest.to_core()),
                Some(RequirementSource::CloudRequirements),
            ),
            approvals_reviewer: ConstrainedWithSource::new(
                Constrained::allow_any(ApprovalsReviewer::AutoReview),
                Some(RequirementSource::LegacyManagedConfigTomlFromMdm),
            ),
            permission_profile: ConstrainedWithSource::new(
                Constrained::allow_any(PermissionProfile::read_only()),
                Some(RequirementSource::SystemRequirementsToml {
                    file: requirements_file.clone(),
                }),
            ),
            mcp_servers: Some(Sourced::new(
                BTreeMap::from([(
                    "docs".to_string(),
                    McpServerRequirement {
                        identity: McpServerIdentity::Command {
                            command: "codex-mcp".to_string(),
                        },
                    },
                )]),
                RequirementSource::LegacyManagedConfigTomlFromMdm,
            )),
            enforce_residency: ConstrainedWithSource::new(
                Constrained::allow_any(Some(ResidencyRequirement::Us)),
                Some(RequirementSource::CloudRequirements),
            ),
            web_search_mode: ConstrainedWithSource::new(
                Constrained::allow_any(WebSearchMode::Cached),
                Some(RequirementSource::CloudRequirements),
            ),
            allow_managed_hooks_only: Some(Sourced::new(
                /*value*/ true,
                RequirementSource::CloudRequirements,
            )),
            feature_requirements: Some(Sourced::new(
                FeatureRequirementsToml {
                    entries: BTreeMap::from([("guardian_approval".to_string(), true)]),
                },
                RequirementSource::CloudRequirements,
            )),
            network: Some(Sourced::new(
                NetworkConstraints {
                    enabled: Some(true),
                    domains: Some(NetworkDomainPermissionsToml {
                        entries: BTreeMap::from([(
                            "example.com".to_string(),
                            NetworkDomainPermissionToml::Allow,
                        )]),
                    }),
                    ..Default::default()
                },
                RequirementSource::CloudRequirements,
            )),
            filesystem: Some(Sourced::new(
                FilesystemConstraints {
                    deny_read: vec![denied_path.clone().into()],
                },
                RequirementSource::SystemRequirementsToml {
                    file: requirements_file.clone(),
                },
            )),
            guardian_policy_config_source: Some(RequirementSource::CloudRequirements),
            ..ConfigRequirements::default()
        };

        let requirements_toml = ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![AskForApproval::OnRequest.to_core()]),
            allowed_approvals_reviewers: Some(vec![ApprovalsReviewer::AutoReview]),
            allowed_sandbox_modes: Some(vec![SandboxModeRequirement::ReadOnly]),
            allowed_permissions: None,
            remote_sandbox_config: None,
            allowed_web_search_modes: Some(vec![WebSearchModeRequirement::Cached]),
            allow_managed_hooks_only: Some(true),
            computer_use: None,
            guardian_policy_config: Some("Use the managed guardian policy.".to_string()),
            feature_requirements: Some(FeatureRequirementsToml {
                entries: BTreeMap::from([("guardian_approval".to_string(), true)]),
            }),
            hooks: None,
            mcp_servers: Some(BTreeMap::from([(
                "docs".to_string(),
                McpServerRequirement {
                    identity: McpServerIdentity::Command {
                        command: "codex-mcp".to_string(),
                    },
                },
            )])),
            plugins: None,
            apps: None,
            rules: None,
            enforce_residency: Some(ResidencyRequirement::Us),
            network: None,
            permissions: None,
        };

        let user_file = if cfg!(windows) {
            absolute_path("C:\\users\\alice\\.codex\\config.toml")
        } else {
            absolute_path("/home/alice/.codex/config.toml")
        };
        let stack = ConfigLayerStack::new(
            vec![ConfigLayerEntry::new(
                ConfigLayerSource::User {
                    file: user_file,
                    profile: None,
                },
                empty_toml_table(),
            )],
            requirements,
            requirements_toml,
        )
        .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(
            rendered.contains("allowed_approval_policies: on-request (source: cloud requirements)")
        );
        assert!(rendered.contains(
            "allowed_approvals_reviewers: guardian_subagent (source: MDM managed_config.toml (legacy))"
        ));
        assert!(
            rendered.contains(
                format!(
                    "allowed_sandbox_modes: read-only (source: {})",
                    requirements_file.as_path().display()
                )
                .as_str(),
            )
        );
        assert!(
            rendered.contains(
                "allowed_web_search_modes: cached, disabled (source: cloud requirements)"
            )
        );
        assert!(rendered.contains("allow_managed_hooks_only: true (source: cloud requirements)"));
        assert!(
            rendered.contains("guardian_policy_config: configured (source: cloud requirements)")
        );
        assert!(rendered.contains("features: guardian_approval=true (source: cloud requirements)"));
        assert!(rendered.contains("mcp_servers: docs (source: MDM managed_config.toml (legacy))"));
        assert!(rendered.contains("enforce_residency: us (source: cloud requirements)"));
        assert!(rendered.contains(
            "experimental_network: enabled=true, domains={example.com=allow} (source: cloud requirements)"
        ));
        assert!(
            rendered.contains(
                format!(
                    "permissions.filesystem.deny_read: {}",
                    denied_path.as_path().display()
                )
                .as_str()
            )
        );
        assert!(!rendered.contains("  - rules:"));
    }

    #[test]
    fn debug_config_output_lists_approvals_reviewer_as_requirement() {
        let requirements = ConfigRequirements {
            approvals_reviewer: ConstrainedWithSource::new(
                Constrained::allow_any(ApprovalsReviewer::AutoReview),
                Some(RequirementSource::LegacyManagedConfigTomlFromMdm),
            ),
            ..ConfigRequirements::default()
        };
        let requirements_toml = ConfigRequirementsToml {
            allowed_approvals_reviewers: Some(vec![ApprovalsReviewer::AutoReview]),
            ..ConfigRequirementsToml::default()
        };
        let stack = ConfigLayerStack::new(Vec::new(), requirements, requirements_toml)
            .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(rendered.contains(
            "allowed_approvals_reviewers: guardian_subagent (source: MDM managed_config.toml (legacy))"
        ));
        assert!(!rendered.contains("Requirements:\n  <none>"));
    }

    #[test]
    fn debug_config_output_formats_unix_socket_permissions() {
        let requirements = ConfigRequirements {
            network: Some(Sourced::new(
                NetworkConstraints {
                    unix_sockets: Some(NetworkUnixSocketPermissionsToml {
                        entries: BTreeMap::from([
                            (
                                "/tmp/codex.sock".to_string(),
                                NetworkUnixSocketPermissionToml::Allow,
                            ),
                            (
                                "/tmp/blocked.sock".to_string(),
                                NetworkUnixSocketPermissionToml::None,
                            ),
                        ]),
                    }),
                    ..Default::default()
                },
                RequirementSource::CloudRequirements,
            )),
            ..ConfigRequirements::default()
        };

        let stack =
            ConfigLayerStack::new(Vec::new(), requirements, ConfigRequirementsToml::default())
                .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(rendered.contains(
            "experimental_network: unix_sockets={/tmp/blocked.sock=none, /tmp/codex.sock=allow} (source: cloud requirements)"
        ));
    }

    #[test]
    fn debug_config_output_lists_session_flag_key_value_pairs() {
        let session_flags = toml::from_str::<TomlValue>(
            r#"
model = "gpt-5"
[sandbox_workspace_write]
network_access = true
writable_roots = ["/tmp"]
"#,
        )
        .expect("session flags");

        let stack = ConfigLayerStack::new(
            vec![ConfigLayerEntry::new(
                ConfigLayerSource::SessionFlags,
                session_flags,
            )],
            ConfigRequirements::default(),
            ConfigRequirementsToml::default(),
        )
        .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(rendered.contains("session-flags (enabled)"));
        assert!(rendered.contains("     - model = \"gpt-5\""));
        assert!(rendered.contains("     - sandbox_workspace_write.network_access = true"));
        assert!(rendered.contains("sandbox_workspace_write.writable_roots"));
        assert!(rendered.contains("/tmp"));
    }

    #[test]
    fn debug_config_output_shows_legacy_mdm_layer_value() {
        let raw_mdm_toml = r#"
# managed by MDM
model = "managed_model"
approval_policy = "never"
"#;
        let mdm_value = toml::from_str::<TomlValue>(raw_mdm_toml).expect("MDM value");

        let stack = ConfigLayerStack::new(
            vec![ConfigLayerEntry::new_with_raw_toml(
                ConfigLayerSource::LegacyManagedConfigTomlFromMdm,
                mdm_value,
                raw_mdm_toml.to_string(),
            )],
            ConfigRequirements::default(),
            ConfigRequirementsToml::default(),
        )
        .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(rendered.contains("legacy managed_config.toml (MDM) (enabled)"));
        assert!(rendered.contains("MDM value:"));
        assert!(rendered.contains("# managed by MDM"));
        assert!(rendered.contains("model = \"managed_model\""));
        assert!(rendered.contains("approval_policy = \"never\""));
    }

    #[test]
    fn debug_config_output_normalizes_empty_web_search_mode_list() {
        let requirements = ConfigRequirements {
            web_search_mode: ConstrainedWithSource::new(
                Constrained::allow_any(WebSearchMode::Disabled),
                Some(RequirementSource::CloudRequirements),
            ),
            ..ConfigRequirements::default()
        };

        let requirements_toml = ConfigRequirementsToml {
            allowed_approval_policies: None,
            allowed_approvals_reviewers: None,
            allowed_sandbox_modes: None,
            allowed_permissions: None,
            remote_sandbox_config: None,
            allowed_web_search_modes: Some(Vec::new()),
            allow_managed_hooks_only: None,
            computer_use: None,
            guardian_policy_config: None,
            feature_requirements: None,
            hooks: None,
            mcp_servers: None,
            plugins: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
            permissions: None,
        };

        let stack = ConfigLayerStack::new(Vec::new(), requirements, requirements_toml)
            .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(
            rendered.contains("allowed_web_search_modes: disabled (source: cloud requirements)")
        );
    }

    #[test]
    fn debug_config_output_lists_managed_hooks_requirement() {
        let requirements = ConfigRequirements {
            managed_hooks: Some(ConstrainedWithSource::new(
                Constrained::allow_any(ManagedHooksRequirementsToml {
                    managed_dir: Some(if cfg!(windows) {
                        std::path::PathBuf::from(r"C:\enterprise\hooks")
                    } else {
                        std::path::PathBuf::from("/enterprise/hooks")
                    }),
                    windows_managed_dir: Some(std::path::PathBuf::from(r"C:\enterprise\hooks")),
                    hooks: HookEventsToml {
                        pre_tool_use: vec![MatcherGroup {
                            matcher: Some("^Bash$".to_string()),
                            hooks: vec![HookHandlerConfig::Command {
                                command: "python3 /enterprise/hooks/pre.py".to_string(),
                                command_windows: None,
                                timeout_sec: Some(10),
                                r#async: false,
                                status_message: Some("checking".to_string()),
                            }],
                        }],
                        ..Default::default()
                    },
                }),
                Some(RequirementSource::CloudRequirements),
            )),
            ..ConfigRequirements::default()
        };
        let requirements_toml = ConfigRequirementsToml {
            hooks: requirements
                .managed_hooks
                .as_ref()
                .map(|hooks| hooks.get().clone()),
            ..ConfigRequirementsToml::default()
        };
        let stack = ConfigLayerStack::new(Vec::new(), requirements, requirements_toml)
            .expect("config layer stack");

        let rendered = render_to_text(&render_debug_config_lines(&stack));
        assert!(rendered.contains("hooks:"));
        assert!(rendered.contains("handlers=1"));
        assert!(rendered.contains("(source: cloud requirements)"));
    }

    #[test]
    fn session_all_proxy_url_uses_socks_when_enabled() {
        assert_eq!(
            session_all_proxy_url(
                "127.0.0.1:3128",
                "127.0.0.1:8081",
                /*socks_enabled*/ true
            ),
            "socks5h://127.0.0.1:8081".to_string()
        );
    }

    #[test]
    fn session_all_proxy_url_uses_http_when_socks_disabled() {
        assert_eq!(
            session_all_proxy_url(
                "127.0.0.1:3128",
                "127.0.0.1:8081",
                /*socks_enabled*/ false
            ),
            "http://127.0.0.1:3128".to_string()
        );
    }
}
