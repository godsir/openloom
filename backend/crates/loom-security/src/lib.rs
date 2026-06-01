// SPDX-License-Identifier: Apache-2.0
//! Security layer — permission check, approval, audit.

use loom_types::{RiskLevel, SkillPermissions};
use lume_skills::SkillPermissionConfig;

/// Check if a tool call is allowed under current permissions.
pub fn check_permission(tool_name: &str, permissions: &SkillPermissions) -> (bool, RiskLevel) {
    // Read-only tools are always Low risk
    let read_only = ["file_read", "file_search", "content_search"];
    if read_only.contains(&tool_name) {
        return (true, RiskLevel::Low);
    }

    // Shell and file-write tools require explicit permission
    if tool_name == "shell" && !permissions.shell {
        return (false, RiskLevel::High);
    }

    // File write tools
    if ["file_write", "file_edit"].contains(&tool_name) && permissions.fs_write.is_none() {
        return (false, RiskLevel::Medium);
    }

    (true, RiskLevel::Low)
}

/// Merge a skill's permissions with global defaults. Most restrictive wins:
/// - Boolean fields (shell, subprocess): if the skill explicitly denies (false),
///   the result is false regardless of defaults. If the skill allows, the default
///   still controls.
/// - Path-list fields (fs_read, fs_write): skill's allowlist is used if specified;
///   otherwise, the default's value is used.
/// - Network: same AND logic as shell.
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
        fs_read: sp.fs_read.clone().or_else(|| defaults.fs_read.clone()),
        fs_write: sp.fs_write.clone().or_else(|| defaults.fs_write.clone()),
        network: sp.network.clone().or_else(|| defaults.network.clone()),
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
