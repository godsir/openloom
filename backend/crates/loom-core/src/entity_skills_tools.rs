//! Skill management tool — CRUD wrapper for SkillState.
//! Each tool delegates to the shared SkillState Arc which is also held by
//! the orchestrator, so mutations are immediately visible.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use loom_types::{ToolDefinition, ToolProgress};
use tokio::sync::RwLock;
use tokio::sync::mpsc::UnboundedSender;

use crate::tool_context::ToolContext;
use crate::tool_registry::{AgentTool, ToolProvenance, ToolResult};

// ============================================================================
// manage_skills
// ============================================================================

pub struct ManageSkillsTool {
    pub skill_state: Arc<RwLock<loom_skills::SkillState>>,
}

#[async_trait]
impl AgentTool for ManageSkillsTool {
    fn tool_name(&self) -> &str {
        "manage_skills"
    }

    fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "manage_skills".into(),
            description: "Manage installed skills (技能). Use when user wants to list skills, import a new skill, delete a skill, or reload all skills from disk.\n\nCommon scenarios:\n- \"show installed skills\": action=list\n- \"import a skill from /path/to/skill-dir\": action=import, source_dir=/path\n- \"delete the pdf skill\": action=delete, name=pdf\n- \"reload all skills after editing\": action=reload\n\nActions: list | import | delete | reload.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "import", "delete", "reload"]
                    },
                    "name": {
                        "type": "string",
                        "description": "Skill name (required for import and delete)"
                    },
                    "source_dir": {
                        "type": "string",
                        "description": "Path to a directory containing SKILL.md (for import). Takes precedence over files array if both provided."
                    },
                    "files": {
                        "type": "array",
                        "description": "Files to write for import. Each element: { path: string, content: string }. At least one file must be named SKILL.md.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "content": { "type": "string" }
                            },
                            "required": ["path", "content"]
                        }
                    }
                },
                "required": ["action"]
            }),
            tags: vec![],
        }
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _progress: UnboundedSender<ToolProgress>,
        _ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let action = arguments["action"].as_str().unwrap_or("");
        let result = exec_manage_skills(action, &arguments, &self.skill_state).await?;
        Ok(ToolResult {
            content: result,
            is_error: false,
            structured_content: None,
        })
    }

    fn provenance(&self) -> ToolProvenance {
        ToolProvenance::Builtin
    }
}

async fn exec_manage_skills(
    action: &str,
    args: &serde_json::Value,
    skill_state: &Arc<RwLock<loom_skills::SkillState>>,
) -> Result<String> {
    match action {
        "list" => {
            let state = skill_state.read().await;
            if state.summaries.is_empty() {
                Ok("No skills installed.".into())
            } else {
                let items: Vec<serde_json::Value> = state
                    .summaries
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "name": s.name,
                            "description": s.description,
                            "source_path": s.source_path,
                            "version": s.version,
                            "user_invocable": s.user_invocable,
                            "always_active": s.always_active,
                        })
                    })
                    .collect();
                Ok(serde_json::to_string_pretty(&serde_json::json!({ "skills": items }))
                    .unwrap_or_else(|e| e.to_string()))
            }
        }
        "import" => {
            let name = args["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("name required for import"))?;

            // Determine destination dir
            let home = dirs::home_dir().unwrap_or_default();
            let skills_root = home.join(".loom").join("skills");
            let skill_dir = skills_root.join(name);

            // Handle source_dir import
            if let Some(source_dir) = args["source_dir"].as_str() {
                let src = std::path::Path::new(source_dir);
                if !src.exists() {
                    return Err(anyhow::anyhow!(
                        "source_dir does not exist: {}",
                        src.display()
                    ));
                }
                let skill_md = src.join("SKILL.md");
                if !skill_md.exists() {
                    return Err(anyhow::anyhow!(
                        "source_dir must contain a SKILL.md file"
                    ));
                }
                copy_dir_recursive(src, &skill_dir)?;
                let count = reload_from_disk(skill_state).await?;
                return Ok(format!(
                    "Skill \"{name}\" imported from {} ({} skills total).",
                    src.display(),
                    count
                ));
            }

            // Handle files-based import
            let files = match args["files"].as_array() {
                Some(f) if !f.is_empty() => f,
                _ => return Err(anyhow::anyhow!(
                    "files array required for import (or provide source_dir)"
                )),
            };

            std::fs::create_dir_all(&skill_dir)
                .map_err(|e| anyhow::anyhow!("mkdir failed: {}", e))?;

            let mut wrote = 0usize;
            let mut has_skill_md = false;
            for file in files {
                let rel_path = file["path"].as_str().unwrap_or("");
                let content = file["content"].as_str().unwrap_or("");
                if rel_path.is_empty() {
                    continue;
                }
                // Prevent path traversal
                if rel_path.contains("..") {
                    continue;
                }
                if rel_path == "SKILL.md"
                    || rel_path.ends_with("/SKILL.md")
                    || rel_path.ends_with("\\SKILL.md")
                {
                    has_skill_md = true;
                }
                let target = skill_dir.join(rel_path);
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(&target, content)
                    .map_err(|e| anyhow::anyhow!("write failed for {}: {}", rel_path, e))?;
                wrote += 1;
            }

            if !has_skill_md {
                // Clean up partial import
                let _ = std::fs::remove_dir_all(&skill_dir);
                return Err(anyhow::anyhow!(
                    "import must include a file named SKILL.md"
                ));
            }

            let count = reload_from_disk(skill_state).await?;
            Ok(format!(
                "Skill \"{name}\" imported ({wrote} files written, {count} skills total)."
            ))
        }
        "delete" => {
            let name = args["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("name required for delete"))?;

            // Resolve the skill's actual on-disk location
            let source_path = {
                let state = skill_state.read().await;
                state
                    .summaries
                    .iter()
                    .find(|s| s.name == name)
                    .map(|s| s.source_path.clone())
            };

            let source_path = match source_path {
                Some(p) => p,
                None => return Ok(format!("Skill \"{name}\" not found.")),
            };

            let skill_md = std::path::Path::new(&source_path);
            let skill_dir = match skill_md.parent() {
                Some(d) => d.to_path_buf(),
                None => {
                    return Err(anyhow::anyhow!("cannot resolve skill directory"));
                }
            };

            // Safety: only delete if the directory is inside a known skill root
            if !is_deletable_skill_dir(&skill_dir) {
                return Err(anyhow::anyhow!(
                    "skill is not in a deletable location"
                ));
            }

            if skill_dir.exists() {
                std::fs::remove_dir_all(&skill_dir)
                    .map_err(|e| anyhow::anyhow!("delete failed: {}", e))?;
            }

            let count = reload_from_disk(skill_state).await?;
            Ok(format!(
                "Skill \"{name}\" deleted ({count} skills remaining)."
            ))
        }
        "reload" => {
            let count = reload_from_disk(skill_state).await?;
            Ok(format!("{count} skills reloaded."))
        }
        _ => Err(anyhow::anyhow!(
            "Unknown action: {action}. Use list | import | delete | reload."
        )),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Reload skills from all standard paths and atomically update the shared
/// SkillState. Returns the new skill count.
async fn reload_from_disk(
    skill_state: &Arc<RwLock<loom_skills::SkillState>>,
) -> Result<usize> {
    let home = dirs::home_dir().unwrap_or_default();
    let data_dir = home.join(".loom");
    let mut loader = loom_skills::SkillLoader::new();
    loader.add_standard_paths(&data_dir);

    let skills = loader
        .discover()
        .map_err(|e| anyhow::anyhow!("skill discovery failed: {}", e))?;
    let count = skills.len();
    let state = loom_skills::SkillState::from_skills(&skills);

    *skill_state.write().await = state;
    Ok(count)
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// A skill directory may only be deleted if it lives STRICTLY inside one of
/// the standard skill roots (user-global or project-local). The skill dir
/// must not be a root itself.
fn is_deletable_skill_dir(dir: &std::path::Path) -> bool {
    let canonical = match dir.canonicalize() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let roots = [
        home.join(".loom").join("skills"),
        home.join(".claude").join("skills"),
        home.join(".openclaw").join("skills"),
        home.join(".codex").join("skills"),
        cwd.join(".loom").join("skills"),
        cwd.join(".claude").join("skills"),
    ];
    roots.iter().any(|r| {
        r.canonicalize()
            .map(|rc| canonical.starts_with(&rc) && canonical != rc)
            .unwrap_or(false)
    })
}
