// SPDX-License-Identifier: Apache-2.0
//! Security layer — permission check, approval, audit, file-system sandbox.

pub mod sandbox;

pub use sandbox::SandboxGuard;

use loom_skills::SkillPermissionConfig;
use loom_types::{RiskLevel, SkillPermissions};

/// Check if a tool call is allowed under current permissions.
/// Returns (allowed, risk_level) where:
/// - allowed: whether the tool may execute (false = blocked)
/// - risk_level: the tool's inherent danger level (used for "ask" mode confirmation)
pub fn check_permission(tool_name: &str, permissions: &SkillPermissions) -> (bool, RiskLevel) {
    // Read-only tools — always allowed, always low risk
    let read_only = ["file_read", "file_search", "content_search", "file_list"];
    if read_only.contains(&tool_name) {
        return (true, RiskLevel::Low);
    }

    // Shell tools — high risk, denied if shell permission is false
    if tool_name == "shell" {
        let allowed = permissions.shell;
        return (allowed, RiskLevel::High);
    }

    // File write/edit/delete — medium risk, denied if fs_write is None
    if ["file_write", "file_edit", "file_delete"].contains(&tool_name) {
        let allowed = permissions.fs_write.is_some();
        return (allowed, RiskLevel::Medium);
    }

    // Scheduled task management — medium risk (creates persistent background jobs)
    if ["schedule_reminder", "create_scheduled_task"].contains(&tool_name) {
        return (true, RiskLevel::Medium);
    }

    // Unknown / meta tools (request_tools, use_skill, web_search, etc.)
    (true, RiskLevel::Low)
}

/// Merge a skill's permissions with global defaults. Most restrictive wins:
/// - Boolean fields (shell, subprocess): if the skill explicitly denies (false),
///   the result is false regardless of defaults. If the skill allows, the default
///   still controls.
/// - Path-list fields (fs_read, fs_write, network): if the base default denies
///   (None), the result is always denied regardless of skill declarations.
///   If the default allows, the skill's allowlist refines it; if unspecified,
///   the default's value is used.
pub fn merge_permissions(
    skill_perms: Option<&SkillPermissionConfig>,
    defaults: &SkillPermissions,
) -> SkillPermissions {
    let Some(sp) = skill_perms else {
        return defaults.clone();
    };
    SkillPermissions {
        shell: sp.shell.unwrap_or(true) && defaults.shell,
        subprocess: sp.subprocess.unwrap_or(true) && defaults.subprocess,
        // Path-list fields: base denial (None) cannot be overridden by skill
        fs_read: if defaults.fs_read.is_none() {
            None
        } else {
            sp.fs_read.clone().or_else(|| defaults.fs_read.clone())
        },
        fs_write: if defaults.fs_write.is_none() {
            None
        } else {
            sp.fs_write.clone().or_else(|| defaults.fs_write.clone())
        },
        network: if defaults.network.is_none() {
            None
        } else {
            sp.network.clone().or_else(|| defaults.network.clone())
        },
    }
}

/// Merge multiple skill permission configs with global defaults. Most
/// restrictive across all skills wins: booleans are AND-ed across all skills
/// and defaults; path lists use the most specific (non-empty allowlist)
/// across all skills, falling back to defaults.
pub fn merge_multi_permissions<'a>(
    skill_perms: impl IntoIterator<Item = Option<&'a SkillPermissionConfig>>,
    defaults: &SkillPermissions,
) -> SkillPermissions {
    let mut result = defaults.clone();
    for sp in skill_perms {
        result = merge_permissions(sp, &result);
    }
    result
}
