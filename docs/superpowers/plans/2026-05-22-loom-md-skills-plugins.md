# loom.md + External Skills + Plugin System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three interconnected features: `loom.md` project instruction files, external SKILL.md-based skill loading with slash command invocation, and a plugin directory scanner compatible with Claude Code's plugin format.

**Architecture:** A `PluginLoader` in the skills crate scans plugin directories for `plugin.json` manifests and `skills/*/SKILL.md` files, producing `ExternalSkill` instances that implement the existing `Skill` trait. A `LoomContext` loader in the engine reads hierarchical `loom.md` files and injects their contents into the system prompt via `detect_project_context()`. External skills surface as `/<skill-name>` slash commands in the TUI, with unrecognized `/` commands falling through to the skill registry before returning "unknown command".

**Tech Stack:** Rust, serde_yaml (new dep for SKILL.md frontmatter), existing SkillRegistry + ContextWeaver + TUI command system.

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `crates/skills/src/external.rs` | `ExternalSkill` struct implementing `Skill` trait from SKILL.md parsing |
| `crates/skills/src/plugin_loader.rs` | `PluginLoader`: scans directories, parses `plugin.json` + `SKILL.md`, returns `Vec<ExternalSkill>` |
| `crates/skills/src/loom_context.rs` | `LoomContext::load()`: reads hierarchical `loom.md` files, returns merged string |

### Modified Files

| File | Changes |
|------|---------|
| `crates/skills/src/lib.rs` | Add `pub mod external; pub mod plugin_loader; pub mod loom_context;` |
| `crates/skills/Cargo.toml` | Add `serde_yaml = "0.9"` dependency |
| `crates/engine/src/lib.rs` | Call `PluginLoader` in `Engine::new()`, inject `loom.md` via `detect_project_context()` |
| `crates/cli/src/tui/commands.rs` | Fall-through: unrecognized `/foo` tries `app.engine.find_skill("foo")` |
| `crates/cli/src/tui/render.rs` | Add external skills to `SLASH_COMMANDS` list dynamically (palette) |

---

### Task 1: Add `serde_yaml` dependency

**Files:**
- Modify: `crates/skills/Cargo.toml`

- [ ] **Step 1: Add serde_yaml to skills crate**

```toml
# In [dependencies], add:
serde_yaml = "0.9"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p openloom-skills`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/skills/Cargo.toml
git commit -m "chore: add serde_yaml dependency for SKILL.md frontmatter parsing"
```

---

### Task 2: `loom_context.rs` — Load hierarchical `loom.md` files

**Files:**
- Create: `crates/skills/src/loom_context.rs`
- Test: inline `#[cfg(test)]` module

The loader reads `loom.md` from two locations (like Claude Code reads CLAUDE.md):
1. **Global:** `<data_dir>/loom.md` (e.g. `%APPDATA%/openLoom/loom.md`)
2. **Project:** `<cwd>/loom.md`

Both are concatenated (global first, then project). If neither exists, returns empty string.

- [ ] **Step 1: Write failing tests**

```rust
// crates/skills/src/loom_context.rs

pub struct LoomContext;

impl LoomContext {
    /// Load loom.md from global data_dir and project cwd, concatenated.
    pub fn load(data_dir: &std::path::Path, cwd: &std::path::Path) -> String {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_no_loom_md_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let result = LoomContext::load(tmp.path(), tmp.path());
        assert!(result.is_empty());
    }

    #[test]
    fn test_project_loom_md_only() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("loom.md"), "# Project Rules\nUse TDD.").unwrap();
        let result = LoomContext::load(data_dir.path(), tmp.path());
        assert!(result.contains("Use TDD"));
    }

    #[test]
    fn test_global_loom_md_only() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("loom.md"), "# Global Config\nBe concise.").unwrap();
        let result = LoomContext::load(tmp.path(), cwd.path());
        assert!(result.contains("Be concise"));
    }

    #[test]
    fn test_both_loom_md_merged() {
        let data_dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        fs::write(data_dir.path().join("loom.md"), "Global instructions.").unwrap();
        fs::write(cwd.path().join("loom.md"), "Project instructions.").unwrap();
        let result = LoomContext::load(data_dir.path(), cwd.path());
        assert!(result.contains("Global instructions"));
        assert!(result.contains("Project instructions"));
        // Global comes before project
        let global_pos = result.find("Global").unwrap();
        let project_pos = result.find("Project").unwrap();
        assert!(global_pos < project_pos);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p openloom-skills -- loom_context`
Expected: FAIL (4 tests, all panicking at `todo!()`)

- [ ] **Step 3: Implement LoomContext::load**

```rust
pub struct LoomContext;

impl LoomContext {
    pub fn load(data_dir: &std::path::Path, cwd: &std::path::Path) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Global loom.md
        let global_path = data_dir.join("loom.md");
        if let Ok(content) = std::fs::read_to_string(&global_path) {
            if !content.trim().is_empty() {
                parts.push(content);
            }
        }

        // Project loom.md (skip if same dir as global to avoid double-loading)
        let project_path = cwd.join("loom.md");
        if project_path != global_path {
            if let Ok(content) = std::fs::read_to_string(&project_path) {
                if !content.trim().is_empty() {
                    parts.push(content);
                }
            }
        }

        parts.join("\n\n")
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p openloom-skills -- loom_context`
Expected: 4 tests PASS

- [ ] **Step 5: Register the module**

In `crates/skills/src/lib.rs`, add near the top:

```rust
pub mod loom_context;
```

- [ ] **Step 6: Commit**

```bash
git add crates/skills/src/loom_context.rs crates/skills/src/lib.rs
git commit -m "feat: add LoomContext loader for hierarchical loom.md files"
```

---

### Task 3: Inject `loom.md` into system prompt

**Files:**
- Modify: `crates/engine/src/lib.rs` (`detect_project_context` function, ~line 118)
- Modify: `crates/engine/src/lib.rs` (`Engine::new`, add `data_dir` field)

The engine already reads `CLAUDE.md` in `detect_project_context()`. We add parallel logic for `loom.md` — but unlike CLAUDE.md, loom.md is NOT truncated to 500 chars (it's meant to be the full instruction set).

- [ ] **Step 1: Add `data_dir` field to Engine struct**

In `crates/engine/src/lib.rs`, add to the `Engine` struct:

```rust
pub struct Engine {
    // ... existing fields ...
    data_dir: PathBuf,
}
```

And in `Engine::new()`, after building the struct, set `data_dir: config.data_dir.clone()`.

- [ ] **Step 2: Add `loom_context` method to Engine**

```rust
impl Engine {
    pub fn loom_context(&self) -> String {
        let cwd = std::env::current_dir().unwrap_or_default();
        openloom_skills::loom_context::LoomContext::load(&self.data_dir, &cwd)
    }
}
```

- [ ] **Step 3: Inject loom.md in detect_project_context()**

In `detect_project_context()`, after the CLAUDE.md block (line ~145), add:

```rust
if cwd.join("loom.md").exists()
    && let Ok(content) = std::fs::read_to_string(cwd.join("loom.md"))
{
    if !content.trim().is_empty() {
        context_parts.push(format!("- Project instructions (loom.md):\n{}", content));
    }
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: OK

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/lib.rs
git commit -m "feat: inject loom.md content into system prompt"
```

---

### Task 4: `external.rs` — ExternalSkill from SKILL.md

**Files:**
- Create: `crates/skills/src/external.rs`
- Test: inline `#[cfg(test)]` module

An `ExternalSkill` is a Skill trait implementation parsed from a SKILL.md file. It holds the parsed frontmatter (name, description) and the full markdown body (context_md). External skills are context-injection-only — they don't execute code, they inject their SKILL.md body into the LLM's context when invoked.

- [ ] **Step 1: Define the SKILL.md frontmatter struct and write failing tests**

```rust
// crates/skills/src/external.rs

use crate::{Skill, SkillManifest};
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    license: Option<String>,
}

pub struct ExternalSkill {
    frontmatter: SkillFrontmatter,
    body: String,
    /// Qualified name: "plugin:skill" (e.g. "superpowers:brainstorming")
    qualified_name: String,
    manifest: OnceLock<SkillManifest>,
}

impl ExternalSkill {
    /// Parse a SKILL.md file. `plugin_name` is prepended to form "plugin:skill".
    pub fn from_skill_md(content: &str, plugin_name: &str) -> Result<Self> {
        todo!()
    }

    pub fn qualified_name(&self) -> &str {
        &self.qualified_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKILL_MD: &str = r#"---
name: brainstorming
description: "Explore user intent before implementation"
---

# Brainstorming

Before implementing, always explore what the user actually needs.

## Steps
1. Ask clarifying questions
2. Propose alternatives
"#;

    #[test]
    fn test_parse_skill_md() {
        let skill = ExternalSkill::from_skill_md(SAMPLE_SKILL_MD, "superpowers").unwrap();
        assert_eq!(skill.qualified_name(), "superpowers:brainstorming");
        assert_eq!(skill.name(), "superpowers:brainstorming");
        assert!(skill.context_md().contains("Before implementing"));
        assert!(skill.manifest().description.contains("Explore user intent"));
    }

    #[test]
    fn test_parse_skill_md_no_frontmatter_fails() {
        let bad = "# Just Markdown\nNo frontmatter here.";
        let result = ExternalSkill::from_skill_md(bad, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_skill_md_with_license() {
        let content = "---\nname: foo\ndescription: bar\nlicense: MIT\n---\nBody text.";
        let skill = ExternalSkill::from_skill_md(content, "myplugin").unwrap();
        assert_eq!(skill.qualified_name(), "myplugin:foo");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p openloom-skills -- external`
Expected: FAIL (3 tests, panicking at `todo!()`)

- [ ] **Step 3: Implement `from_skill_md` and Skill trait**

```rust
impl ExternalSkill {
    pub fn from_skill_md(content: &str, plugin_name: &str) -> Result<Self> {
        let content = content.trim();
        if !content.starts_with("---") {
            anyhow::bail!("SKILL.md must start with YAML frontmatter (---)");
        }

        let end = content[3..]
            .find("\n---")
            .ok_or_else(|| anyhow::anyhow!("No closing --- for frontmatter"))?;
        let yaml_str = &content[3..3 + end].trim();
        let body = content[3 + end + 4..].trim().to_string(); // skip "\n---"

        let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_str)?;
        let qualified_name = format!("{}:{}", plugin_name, frontmatter.name);

        Ok(Self {
            frontmatter,
            body,
            qualified_name,
            manifest: OnceLock::new(),
        })
    }

    pub fn qualified_name(&self) -> &str {
        &self.qualified_name
    }
}

#[async_trait::async_trait]
impl Skill for ExternalSkill {
    fn name(&self) -> &str {
        &self.qualified_name
    }

    fn manifest(&self) -> &SkillManifest {
        self.manifest.get_or_init(|| SkillManifest {
            name: self.qualified_name.clone(),
            description: self.frontmatter.description.clone(),
            triggers: Vec::new(),
            permissions: crate::SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, _params: Value) -> Result<Value> {
        // External skills are context-injection-only.
        // Invoking them returns their full body for the LLM to follow.
        Ok(serde_json::json!({
            "skill": self.qualified_name,
            "context": self.body,
        }))
    }

    fn context_md(&self) -> &str {
        &self.body
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p openloom-skills -- external`
Expected: 3 tests PASS

- [ ] **Step 5: Register the module**

In `crates/skills/src/lib.rs`, add:

```rust
pub mod external;
```

- [ ] **Step 6: Commit**

```bash
git add crates/skills/src/external.rs crates/skills/src/lib.rs
git commit -m "feat: add ExternalSkill parsed from SKILL.md frontmatter"
```

---

### Task 5: `plugin_loader.rs` — Scan plugin directories

**Files:**
- Create: `crates/skills/src/plugin_loader.rs`
- Test: inline `#[cfg(test)]` module

The `PluginLoader` scans a directory tree for plugins. Each plugin is a directory containing `.claude-plugin/plugin.json` (or `.loom-plugin/plugin.json`) and a `skills/*/SKILL.md` tree. It also supports a flat `skills/` directory directly under the data dir for user-created skills without a full plugin wrapper.

Scan locations (in order):
1. `<data_dir>/plugins/` — user-installed plugins (each subdirectory is a plugin)
2. `<cwd>/.loom/skills/` — project-local skills (flat, no plugin.json needed; plugin_name = "project")

- [ ] **Step 1: Define PluginInfo and write failing tests**

```rust
// crates/skills/src/plugin_loader.rs

use crate::external::ExternalSkill;
use anyhow::Result;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
}

pub struct PluginLoader;

impl PluginLoader {
    /// Scan all plugin locations and return discovered external skills.
    pub fn discover(data_dir: &Path, cwd: &Path) -> Vec<ExternalSkill> {
        todo!()
    }

    /// Scan a single plugin directory (has .claude-plugin/plugin.json or .loom-plugin/plugin.json
    /// and skills/*/SKILL.md).
    fn load_plugin(plugin_dir: &Path) -> Result<(PluginManifest, Vec<ExternalSkill>)> {
        todo!()
    }

    /// Scan a flat skills directory (each subdirectory has a SKILL.md, no plugin.json needed).
    fn load_flat_skills(skills_dir: &Path, plugin_name: &str) -> Vec<ExternalSkill> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_plugin(root: &Path, plugin_name: &str, skills: &[(&str, &str)]) {
        let plugin_dir = root.join(plugin_name);
        let manifest_dir = plugin_dir.join(".loom-plugin");
        fs::create_dir_all(&manifest_dir).unwrap();
        let manifest = format!(
            r#"{{"name":"{}","description":"Test plugin"}}"#,
            plugin_name
        );
        fs::write(manifest_dir.join("plugin.json"), manifest).unwrap();

        for (skill_name, body) in skills {
            let skill_dir = plugin_dir.join("skills").join(skill_name);
            fs::create_dir_all(&skill_dir).unwrap();
            let content = format!(
                "---\nname: {}\ndescription: Test skill {}\n---\n{}",
                skill_name, skill_name, body
            );
            fs::write(skill_dir.join("SKILL.md"), content).unwrap();
        }
    }

    #[test]
    fn test_discover_no_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = PluginLoader::discover(tmp.path(), tmp.path());
        assert!(skills.is_empty());
    }

    #[test]
    fn test_discover_plugin_with_skills() {
        let data_dir = tempfile::tempdir().unwrap();
        let plugins_dir = data_dir.path().join("plugins");
        fs::create_dir_all(&plugins_dir).unwrap();
        setup_plugin(&plugins_dir, "myplugin", &[("greet", "Say hello.")]);

        let cwd = tempfile::tempdir().unwrap();
        let skills = PluginLoader::discover(data_dir.path(), cwd.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].qualified_name(), "myplugin:greet");
    }

    #[test]
    fn test_discover_project_local_skills() {
        let data_dir = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let skills_dir = cwd.path().join(".loom").join("skills");
        let skill_dir = skills_dir.join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Local skill\n---\nDo the thing.",
        )
        .unwrap();

        let skills = PluginLoader::discover(data_dir.path(), cwd.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].qualified_name(), "project:my-skill");
    }

    #[test]
    fn test_discover_claude_plugin_format() {
        let data_dir = tempfile::tempdir().unwrap();
        let plugins_dir = data_dir.path().join("plugins");
        let plugin_dir = plugins_dir.join("test-plugin");
        let manifest_dir = plugin_dir.join(".claude-plugin");
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::write(
            manifest_dir.join("plugin.json"),
            r#"{"name":"test-plugin","description":"Claude format"}"#,
        )
        .unwrap();
        let skill_dir = plugin_dir.join("skills").join("debug");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: debug\ndescription: Debug skill\n---\nDebug instructions.",
        )
        .unwrap();

        let cwd = tempfile::tempdir().unwrap();
        let skills = PluginLoader::discover(data_dir.path(), cwd.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].qualified_name(), "test-plugin:debug");
    }

    #[test]
    fn test_load_plugin_reads_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        setup_plugin(tmp.path(), "myplugin", &[("greet", "Say hello.")]);
        let (manifest, skills) = PluginLoader::load_plugin(&tmp.path().join("myplugin")).unwrap();
        assert_eq!(manifest.name, "myplugin");
        assert_eq!(skills.len(), 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p openloom-skills -- plugin_loader`
Expected: FAIL (5 tests, all panicking at `todo!()`)

- [ ] **Step 3: Implement PluginLoader**

```rust
impl PluginLoader {
    pub fn discover(data_dir: &Path, cwd: &Path) -> Vec<ExternalSkill> {
        let mut skills = Vec::new();

        // 1. Scan <data_dir>/plugins/*/
        let plugins_dir = data_dir.join("plugins");
        if plugins_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        match Self::load_plugin(&path) {
                            Ok((_manifest, plugin_skills)) => {
                                skills.extend(plugin_skills);
                            }
                            Err(e) => {
                                tracing::debug!(path = %path.display(), err = %e, "Skipping plugin");
                            }
                        }
                    }
                }
            }
        }

        // 2. Scan <cwd>/.loom/skills/*/
        let project_skills_dir = cwd.join(".loom").join("skills");
        if project_skills_dir.is_dir() {
            skills.extend(Self::load_flat_skills(&project_skills_dir, "project"));
        }

        skills
    }

    fn load_plugin(plugin_dir: &Path) -> Result<(PluginManifest, Vec<ExternalSkill>)> {
        // Try .loom-plugin/plugin.json first, then .claude-plugin/plugin.json
        let manifest_path = [".loom-plugin", ".claude-plugin"]
            .iter()
            .map(|d| plugin_dir.join(d).join("plugin.json"))
            .find(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("No plugin.json found in {:?}", plugin_dir))?;

        let manifest_str = std::fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_json::from_str(&manifest_str)?;

        let skills_dir = plugin_dir.join("skills");
        let skills = if skills_dir.is_dir() {
            Self::load_flat_skills(&skills_dir, &manifest.name)
        } else {
            Vec::new()
        };

        Ok((manifest, skills))
    }

    fn load_flat_skills(skills_dir: &Path, plugin_name: &str) -> Vec<ExternalSkill> {
        let mut skills = Vec::new();
        if let Ok(entries) = std::fs::read_dir(skills_dir) {
            for entry in entries.flatten() {
                let skill_dir = entry.path();
                if !skill_dir.is_dir() {
                    continue;
                }
                let skill_md = skill_dir.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }
                match std::fs::read_to_string(&skill_md) {
                    Ok(content) => match ExternalSkill::from_skill_md(&content, plugin_name) {
                        Ok(skill) => skills.push(skill),
                        Err(e) => {
                            tracing::debug!(path = %skill_md.display(), err = %e, "Skipping skill");
                        }
                    },
                    Err(e) => {
                        tracing::debug!(path = %skill_md.display(), err = %e, "Cannot read skill");
                    }
                }
            }
        }
        skills
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p openloom-skills -- plugin_loader`
Expected: 5 tests PASS

- [ ] **Step 5: Register the module and add tempfile dev-dependency**

In `crates/skills/src/lib.rs`, add:

```rust
pub mod plugin_loader;
```

In `crates/skills/Cargo.toml`, add to `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 6: Commit**

```bash
git add crates/skills/src/plugin_loader.rs crates/skills/src/lib.rs crates/skills/Cargo.toml
git commit -m "feat: add PluginLoader for scanning plugin directories and SKILL.md files"
```

---

### Task 6: Wire PluginLoader into Engine startup

**Files:**
- Modify: `crates/engine/src/lib.rs` (`Engine::new()`, ~line 246)

After registering built-in skills, call `PluginLoader::discover()` and register each `ExternalSkill` into the registry.

- [ ] **Step 1: Add plugin loading after builtin registration**

In `Engine::new()`, after line 251 (`router.register_skill_triggers(...)`), add:

```rust
// Load external skills from plugins
let cwd = std::env::current_dir().unwrap_or_default();
let external_skills = openloom_skills::plugin_loader::PluginLoader::discover(
    &config.data_dir,
    &cwd,
);
for ext_skill in external_skills {
    tracing::info!(name = ext_skill.qualified_name(), "Loaded external skill");
    let manifest = ext_skill.manifest().clone();
    let name = ext_skill.qualified_name().to_string();
    skills.register(Box::new(ext_skill));
    if !manifest.triggers.is_empty() {
        router.register_skill_triggers(&name, manifest.triggers);
    }
}
```

- [ ] **Step 2: Store data_dir on Engine**

Add `data_dir: config.data_dir.clone(),` to the Engine struct initialization.

- [ ] **Step 3: Add public method to list external skill names**

```rust
impl Engine {
    pub fn external_skill_names(&self) -> Vec<String> {
        self.skills
            .list_all()
            .iter()
            .filter(|s| s.name.contains(':'))
            .map(|s| s.name.clone())
            .collect()
    }
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: OK

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/lib.rs
git commit -m "feat: wire PluginLoader into Engine startup, register external skills"
```

---

### Task 7: Slash command fall-through to skills

**Files:**
- Modify: `crates/cli/src/tui/commands.rs` (`parse_slash_command` and `execute_command`)

When the user types `/<anything>` that isn't a built-in slash command, we check if it matches a registered skill name. If so, we invoke that skill and inject its context into the conversation. This is how Claude Code's `/<skill-name>` works.

- [ ] **Step 1: Add SkillInvoke variant to SlashCommand**

```rust
pub enum SlashCommand {
    // ... existing variants ...
    SkillInvoke(String), // skill qualified name
}
```

- [ ] **Step 2: Modify parse_slash_command to return SkillInvoke for unknowns**

Change the `_ => None` at the end of `parse_slash_command` to return `SkillInvoke` instead:

```rust
_ => Some(SlashCommand::SkillInvoke(input[1..].to_string())),
```

This means ALL unrecognized `/foo` commands become skill invocation attempts. The execute handler will check if the skill exists and return "unknown command" if it doesn't.

- [ ] **Step 3: Handle SkillInvoke in execute_command**

Add at the end of `execute_command`'s match:

```rust
SlashCommand::SkillInvoke(ref name) => {
    // Try exact match first, then try with common plugin prefixes
    let candidates = [
        name.clone(),
        format!("project:{}", name),
        format!("superpowers:{}", name),
    ];

    for candidate in &candidates {
        if let Some(skill) = app.engine.find_skill_by_name(candidate) {
            let context = skill.to_string();
            app.add_assistant_message(format!(
                "Using **{}** skill.\n\n{}",
                candidate,
                // Show first 200 chars as preview
                if context.len() > 200 {
                    format!("{}...", &context[..200])
                } else {
                    context
                }
            ));
            return format!("[Skill {} activated]", candidate);
        }
    }

    format!(
        "Unknown command: /{}. Type /help for available commands, /skills list for skills.",
        name
    )
}
```

- [ ] **Step 4: Add find_skill_by_name to Engine**

In `crates/engine/src/lib.rs`:

```rust
impl Engine {
    pub fn find_skill_by_name(&self, name: &str) -> Option<String> {
        self.skills
            .find_by_name(name)
            .map(|s| s.context_md().to_string())
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check`
Expected: OK

- [ ] **Step 6: Commit**

```bash
git add crates/cli/src/tui/commands.rs crates/engine/src/lib.rs
git commit -m "feat: slash command fall-through to skill invocation"
```

---

### Task 8: Dynamic command palette for external skills

**Files:**
- Modify: `crates/cli/src/tui/render.rs` (`palette_matches` function)
- Modify: `crates/cli/src/tui/app.rs` (add `external_commands` field)

The command palette currently shows only `SLASH_COMMANDS` (static list). We need to also show external skills when the user types `/`.

- [ ] **Step 1: Add external_commands to App**

In `crates/cli/src/tui/app.rs`, add to App struct:

```rust
pub struct App {
    // ... existing fields ...
    /// External skill names populated at startup for command palette
    pub external_commands: Vec<(String, String)>, // (name, description)
}
```

Initialize it in `App::new()`:

```rust
external_commands: Vec::new(),
```

- [ ] **Step 2: Populate external_commands after Engine init**

In `crates/cli/src/tui/mod.rs`, after creating `App`, populate the external commands:

```rust
// Populate external skill commands for palette
let all_skills = app.engine.list_skills();
for skill in &all_skills {
    if skill.name.contains(':') {
        // Extract short name (after colon) for slash command display
        let short = skill.name.split(':').last().unwrap_or(&skill.name);
        app.external_commands.push((
            format!("/{}", short),
            skill.description.clone(),
        ));
    }
}
```

- [ ] **Step 3: Modify palette_matches to include external commands**

In `crates/cli/src/tui/render.rs`, change `palette_matches` to accept the App's external commands:

```rust
pub fn palette_matches_with_externals(
    input: &str,
    external_commands: &[(String, String)],
) -> Vec<(String, String)> {
    if !input.starts_with('/') || input.starts_with("//") {
        return Vec::new();
    }

    let typed = &input[1..];
    let typed_lower = typed.to_lowercase();

    // Check exact match against builtins
    let has_exact = SLASH_COMMANDS
        .iter()
        .any(|(cmd, _)| cmd[1..].eq_ignore_ascii_case(typed_lower.trim()));
    if has_exact && !typed_lower.trim().is_empty() {
        return Vec::new();
    }

    let mut results: Vec<(String, String)> = Vec::new();

    // Built-in commands
    for (cmd, desc) in SLASH_COMMANDS {
        let full = &cmd[1..];
        if full.starts_with(&typed_lower) || typed_lower.is_empty() {
            results.push((cmd.to_string(), desc.to_string()));
        }
    }

    // External skill commands
    for (cmd, desc) in external_commands {
        let full = &cmd[1..];
        if full.starts_with(&typed_lower) || typed_lower.is_empty() {
            results.push((cmd.clone(), desc.clone()));
        }
    }

    results
}
```

Update all call sites (in `render.rs` and `input.rs`) to use the new function, passing `&app.external_commands`.

- [ ] **Step 4: Verify compilation and tests**

Run: `cargo check && cargo test -p openloom`
Expected: OK, all 31 CLI tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/cli/src/tui/app.rs crates/cli/src/tui/mod.rs crates/cli/src/tui/render.rs crates/cli/src/tui/input.rs
git commit -m "feat: show external skills in command palette"
```

---

### Task 9: Integration test — end-to-end plugin loading

**Files:**
- Create: `tests/test_plugins.rs`

- [ ] **Step 1: Write integration test**

```rust
// tests/test_plugins.rs

use std::fs;

#[test]
fn test_external_skill_parsing() {
    let content = r#"---
name: test-skill
description: "A test skill for integration testing"
---

# Test Skill

When invoked, follow these steps:
1. Do thing A
2. Do thing B
"#;

    let skill = openloom_skills::external::ExternalSkill::from_skill_md(content, "test-plugin")
        .expect("Should parse valid SKILL.md");
    assert_eq!(skill.qualified_name(), "test-plugin:test-skill");

    use openloom_skills::Skill;
    assert!(skill.context_md().contains("Do thing A"));
    assert!(skill.manifest().description.contains("test skill"));
}

#[test]
fn test_plugin_loader_discovers_skills() {
    let data_dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();

    // Create a project-local skill
    let skill_dir = cwd.path().join(".loom").join("skills").join("my-tool");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: my-tool\ndescription: My custom tool\n---\nUse this tool wisely.",
    )
    .unwrap();

    let skills = openloom_skills::plugin_loader::PluginLoader::discover(
        data_dir.path(),
        cwd.path(),
    );
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].qualified_name(), "project:my-tool");
}

#[test]
fn test_loom_context_loading() {
    let data_dir = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();

    fs::write(
        cwd.path().join("loom.md"),
        "# My Project\n\nAlways use TDD.\nPrefer Rust.",
    )
    .unwrap();

    let context = openloom_skills::loom_context::LoomContext::load(data_dir.path(), cwd.path());
    assert!(context.contains("Always use TDD"));
    assert!(context.contains("Prefer Rust"));
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test test_plugins`
Expected: 3 tests PASS

- [ ] **Step 3: Commit**

```bash
git add tests/test_plugins.rs
git commit -m "test: add integration tests for plugin loading and loom.md"
```

---

### Task 10: Update `/help` and documentation

**Files:**
- Modify: `crates/cli/src/tui/commands.rs` (help text)
- Modify: `docs/tui-usage.md`

- [ ] **Step 1: Update help overlay text**

In the help command handler, add sections for loom.md and skills:

```
Skills & Plugins:
  /skills list          List all registered skills
  /skills invoke        Invoke a skill by name
  /<skill-name>         Invoke a skill directly (e.g. /brainstorming)

  Skills are loaded from:
  • <data_dir>/plugins/*/skills/*/SKILL.md
  • <cwd>/.loom/skills/*/SKILL.md

Project Instructions:
  Place a loom.md file in your project root.
  Contents are injected into every prompt as project context.
  Global instructions: <data_dir>/loom.md
```

- [ ] **Step 2: Update docs/tui-usage.md**

Add a section documenting loom.md, skills, and plugins.

- [ ] **Step 3: Verify compilation**

Run: `cargo check && cargo test -p openloom`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/cli/src/tui/commands.rs docs/tui-usage.md
git commit -m "docs: document loom.md, external skills, and plugin system"
```

---

## Summary

| Task | Deliverable |
|------|------------|
| 1 | serde_yaml dependency |
| 2 | `LoomContext::load()` with 4 tests |
| 3 | loom.md injection into system prompt |
| 4 | `ExternalSkill` from SKILL.md with 3 tests |
| 5 | `PluginLoader::discover()` with 5 tests |
| 6 | Plugin loading wired into Engine startup |
| 7 | Slash command fall-through to skills |
| 8 | Dynamic command palette for external skills |
| 9 | Integration tests (3 tests) |
| 10 | Help text and documentation |

Total new tests: 15 (4 + 3 + 5 + 3 integration)
