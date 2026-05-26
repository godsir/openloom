// SPDX-License-Identifier: Apache-2.0
//! Security layer — permission check, approval, audit.

use loom_types::{RiskLevel, SkillPermissions};

/// Check if a tool call is allowed under current permissions.
pub fn check_permission(
    tool_name: &str,
    permissions: &SkillPermissions,
) -> (bool, RiskLevel) {
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
    if ["file_write", "file_edit"].contains(&tool_name) {
        if permissions.fs_write.is_none() {
            return (false, RiskLevel::Medium);
        }
    }

    (true, RiskLevel::Low)
}
