//! Skill permission types.
//!
//! Consumers: loom-core (permission check), loom-skills, loom-security

use serde::{Deserialize, Serialize};

/// Permissions granted to a skill or tool.
///
/// Consumers: loom-core (agent loop permission check), loom-skills (SkillManifest), loom-security (sandbox)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillPermissions {
    #[serde(default)]
    pub fs_read: Option<Vec<String>>,
    #[serde(default)]
    pub fs_write: Option<Vec<String>>,
    #[serde(default)]
    pub network: Option<Vec<String>>,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub subprocess: bool,
}
