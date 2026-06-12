//! Default marketplace catalog — a curated list of community plugins and skills.
//!
//! This catalog ships with the application. In the future, it can be
//! extended to fetch from a remote JSON endpoint for live updates.

use serde::{Deserialize, Serialize};

/// The kind of marketplace entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MarketEntryKind {
    Plugin,
    Skill,
}

impl std::fmt::Display for MarketEntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketEntryKind::Plugin => write!(f, "plugin"),
            MarketEntryKind::Skill => write!(f, "skill"),
        }
    }
}

/// A marketplace entry — can be a plugin or a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPlugin {
    /// Unique identifier (kebab-case).
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// One-line description.
    pub description: String,
    /// Latest available version in the marketplace.
    pub version: String,
    /// Author or maintainer.
    pub author: String,
    /// Git clone URL — GitHub, GitLab, or any git remote.
    pub git_url: String,
    /// Category for grouping in the UI.
    pub category: String,
    /// Entry kind: plugin (hooks + MCP + skills) or standalone skill.
    #[serde(default = "default_kind")]
    pub kind: MarketEntryKind,
    /// Search tags.
    pub tags: Vec<String>,
    /// Optional project homepage or documentation URL.
    pub homepage: Option<String>,
}

fn default_kind() -> MarketEntryKind {
    MarketEntryKind::Plugin
}

/// The marketplace catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceCatalog {
    pub plugins: Vec<MarketPlugin>,
}

/// Return the default built-in catalog.
pub fn default_catalog() -> MarketplaceCatalog {
    MarketplaceCatalog {
        plugins: vec![
            // ════════════════════════════════════════════════════════════
            // Plugins
            // ════════════════════════════════════════════════════════════

            // ── Official Plugins (Anthropic) ──
            MarketPlugin {
                id: "security-guidance".into(),
                name: "Security Guidance".into(),
                description: "Security reminder hook that warns about potential security issues when editing files".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/claude-plugins-official".into(),
                category: "Security".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["security".into(), "hooks".into(), "reminder".into()],
                homepage: Some("https://github.com/anthropics/claude-plugins-official".into()),
            },
            MarketPlugin {
                id: "code-reviewer".into(),
                name: "Code Reviewer".into(),
                description: "Automated code review hooks — runs linting and static analysis before file edits".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/claude-plugins-official".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["code-review".into(), "linting".into(), "hooks".into()],
                homepage: Some("https://github.com/anthropics/claude-plugins-official".into()),
            },
            MarketPlugin {
                id: "context-window-manager".into(),
                name: "Context Window Manager".into(),
                description: "Monitors token usage and warns before hitting context limits".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/claude-plugins-official".into(),
                category: "Productivity".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["tokens".into(), "context".into(), "monitoring".into()],
                homepage: Some("https://github.com/anthropics/claude-plugins-official".into()),
            },

            // ── Community Plugins ──
            MarketPlugin {
                id: "shell-safety".into(),
                name: "Shell Safety".into(),
                description: "Intercepts shell commands and adds safety confirmations for destructive operations (rm -rf, force push, etc.)".into(),
                version: "0.2.0".into(),
                author: "Community".into(),
                git_url: "https://github.com/godsir/claude-shell-safety".into(),
                category: "Security".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["shell".into(), "safety".into(), "confirmation".into(), "hooks".into()],
                homepage: Some("https://github.com/godsir/claude-shell-safety".into()),
            },
            MarketPlugin {
                id: "project-context".into(),
                name: "Project Context".into(),
                description: "Automatically injects project README, architecture docs, and conventions into the conversation context".into(),
                version: "0.1.0".into(),
                author: "Community".into(),
                git_url: "https://github.com/godsir/claude-project-context".into(),
                category: "Productivity".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["context".into(), "project".into(), "docs".into(), "prompt".into()],
                homepage: Some("https://github.com/godsir/claude-project-context".into()),
            },
            MarketPlugin {
                id: "session-summarizer".into(),
                name: "Session Summarizer".into(),
                description: "Generates structured session summaries with key decisions, action items, and file changes".into(),
                version: "0.2.1".into(),
                author: "Community".into(),
                git_url: "https://github.com/godsir/claude-session-summarizer".into(),
                category: "Productivity".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["session".into(), "summary".into(), "notes".into(), "hooks".into()],
                homepage: Some("https://github.com/godsir/claude-session-summarizer".into()),
            },
            MarketPlugin {
                id: "git-worktree-helper".into(),
                name: "Git Worktree Helper".into(),
                description: "Manages git worktrees for parallel agent tasks — creates, cleans up, and tracks isolated workspaces".into(),
                version: "0.1.0".into(),
                author: "Community".into(),
                git_url: "https://github.com/godsir/claude-git-worktree".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["git".into(), "worktree".into(), "isolation".into()],
                homepage: Some("https://github.com/godsir/claude-git-worktree".into()),
            },
            MarketPlugin {
                id: "todo-tracker".into(),
                name: "Todo Tracker".into(),
                description: "Tracks TODO items across sessions, generates task lists, and follows up on incomplete items".into(),
                version: "0.3.0".into(),
                author: "Community".into(),
                git_url: "https://github.com/godsir/claude-todo-tracker".into(),
                category: "Productivity".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["todo".into(), "tasks".into(), "tracking".into()],
                homepage: Some("https://github.com/godsir/claude-todo-tracker".into()),
            },

            // ── Built-in MCP Plugins ──
            MarketPlugin {
                id: "github".into(),
                name: "GitHub".into(),
                description: "GitHub integration — manage issues, PRs, repositories, and code search via MCP. Requires a GitHub personal access token.".into(),
                version: "1.0.0".into(),
                author: "openLoom".into(),
                git_url: "https://github.com/anthropics/github-mcp".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["github".into(), "git".into(), "issues".into(), "pr".into(), "mcp".into()],
                homepage: Some("https://github.com/anthropics/github-mcp".into()),
            },
            MarketPlugin {
                id: "context7".into(),
                name: "Context7".into(),
                description: "Fetch up-to-date documentation and library references at query time — resolves latest docs by pulling real documentation snippets into context.".into(),
                version: "1.0.0".into(),
                author: "Context7".into(),
                git_url: "https://github.com/context7/context7-mcp".into(),
                category: "Research".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["docs".into(), "reference".into(), "mcp".into(), "context".into()],
                homepage: Some("https://github.com/context7/context7-mcp".into()),
            },
            MarketPlugin {
                id: "playwright".into(),
                name: "Playwright".into(),
                description: "Browser automation — navigate pages, take screenshots, fill forms, test web apps. Uses headless Chromium via @playwright/mcp.".into(),
                version: "1.0.0".into(),
                author: "Microsoft".into(),
                git_url: "https://github.com/microsoft/playwright-mcp".into(),
                category: "Testing".into(),
                kind: MarketEntryKind::Plugin,
                tags: vec!["browser".into(), "testing".into(), "automation".into(), "screenshot".into(), "mcp".into()],
                homepage: Some("https://github.com/microsoft/playwright-mcp".into()),
            },

            // ════════════════════════════════════════════════════════════
            // Skills
            // ════════════════════════════════════════════════════════════

            MarketPlugin {
                id: "brainstorming".into(),
                name: "Brainstorming".into(),
                description: "Structured brainstorming workflow — explores user intent, requirements, and design before any implementation".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/skills-official".into(),
                category: "Workflow".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["brainstorming".into(), "planning".into(), "design".into(), "workflow".into()],
                homepage: Some("https://github.com/anthropics/skills-official".into()),
            },
            MarketPlugin {
                id: "systematic-debugging".into(),
                name: "Systematic Debugging".into(),
                description: "Structured debugging workflow — root-cause analysis with verification steps, no guesswork".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/skills-official".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["debugging".into(), "troubleshooting".into(), "workflow".into()],
                homepage: Some("https://github.com/anthropics/skills-official".into()),
            },
            MarketPlugin {
                id: "tdd-workflow".into(),
                name: "TDD Workflow".into(),
                description: "Test-driven development workflow — write tests first, then implement, always verify before completion".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/skills-official".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["tdd".into(), "testing".into(), "workflow".into()],
                homepage: Some("https://github.com/anthropics/skills-official".into()),
            },
            MarketPlugin {
                id: "code-review-skill".into(),
                name: "Code Review".into(),
                description: "Comprehensive code review workflow — checks for bugs, security issues, performance, and code quality".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/skills-official".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["code-review".into(), "quality".into(), "security".into()],
                homepage: Some("https://github.com/anthropics/skills-official".into()),
            },
            MarketPlugin {
                id: "frontend-design".into(),
                name: "Frontend Design".into(),
                description: "Create distinctive, production-grade frontend interfaces with high design quality — no generic AI aesthetics".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/skills-official".into(),
                category: "Design".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["frontend".into(), "design".into(), "ui".into(), "ux".into()],
                homepage: Some("https://github.com/anthropics/skills-official".into()),
            },
            MarketPlugin {
                id: "deep-research".into(),
                name: "Deep Research".into(),
                description: "Multi-source research with adversarial verification — fan-out web searches, cross-check claims, produce cited reports".into(),
                version: "1.0.0".into(),
                author: "Anthropic".into(),
                git_url: "https://github.com/anthropics/skills-official".into(),
                category: "Research".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["research".into(), "web-search".into(), "verification".into()],
                homepage: Some("https://github.com/anthropics/skills-official".into()),
            },
            MarketPlugin {
                id: "project-bootstrap".into(),
                name: "Project Bootstrap".into(),
                description: "Initialize new projects with best practices — scaffolding, config files, CI setup, and documentation templates".into(),
                version: "0.2.0".into(),
                author: "Community".into(),
                git_url: "https://github.com/godsir/claude-project-bootstrap".into(),
                category: "Workflow".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["scaffolding".into(), "project-setup".into(), "templates".into()],
                homepage: Some("https://github.com/godsir/claude-project-bootstrap".into()),
            },
            MarketPlugin {
                id: "api-builder".into(),
                name: "API Builder".into(),
                description: "Design and implement REST/GraphQL APIs — OpenAPI specs, route generation, validation, and client SDK generation".into(),
                version: "0.1.0".into(),
                author: "Community".into(),
                git_url: "https://github.com/godsir/claude-api-builder".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["api".into(), "rest".into(), "graphql".into(), "openapi".into()],
                homepage: Some("https://github.com/godsir/claude-api-builder".into()),
            },
            MarketPlugin {
                id: "loom-code-review".into(),
                name: "Loom Code Review".into(),
                description: "Comprehensive code review — checks for bugs, security issues, performance problems, and code quality with structured severity ratings.".into(),
                version: "1.0.0".into(),
                author: "openLoom".into(),
                git_url: "https://github.com/openloom/builtin-skills".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["loom-code-review".into(), "code-review".into(), "quality".into(), "security".into()],
                homepage: Some("https://github.com/openloom/builtin-skills".into()),
            },
            MarketPlugin {
                id: "loom-bug-hunt".into(),
                name: "Loom Bug Hunt".into(),
                description: "Systematic bug hunting — deep investigation of code to find hidden bugs, edge cases, and reliability issues that automated tools miss.".into(),
                version: "1.0.0".into(),
                author: "openLoom".into(),
                git_url: "https://github.com/openloom/builtin-skills".into(),
                category: "Development".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["loom-bug-hunt".into(), "testing".into(), "qa".into(), "debugging".into()],
                homepage: Some("https://github.com/openloom/builtin-skills".into()),
            },
            MarketPlugin {
                id: "loom-frontend-polish".into(),
                name: "Loom Frontend Polish".into(),
                description: "Polish frontend interfaces to production quality — improve UX, accessibility, responsiveness, and visual design to eliminate generic AI aesthetics.".into(),
                version: "1.0.0".into(),
                author: "openLoom".into(),
                git_url: "https://github.com/openloom/builtin-skills".into(),
                category: "Design".into(),
                kind: MarketEntryKind::Skill,
                tags: vec!["loom-frontend-polish".into(), "frontend".into(), "design".into(), "ui".into(), "ux".into(), "a11y".into()],
                homepage: Some("https://github.com/openloom/builtin-skills".into()),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_has_required_fields() {
        let catalog = default_catalog();
        for p in &catalog.plugins {
            assert!(!p.id.is_empty());
            assert!(!p.name.is_empty());
            assert!(!p.git_url.is_empty());
            assert!(!p.category.is_empty());
        }
    }

    #[test]
    fn test_catalog_ids_are_unique() {
        let catalog = default_catalog();
        let mut ids: Vec<&str> = catalog.plugins.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        let orig_len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), orig_len, "catalog entry IDs must be unique");
    }

    #[test]
    fn test_catalog_has_both_kinds() {
        let catalog = default_catalog();
        let plugins = catalog
            .plugins
            .iter()
            .filter(|p| p.kind == MarketEntryKind::Plugin)
            .count();
        let skills = catalog
            .plugins
            .iter()
            .filter(|p| p.kind == MarketEntryKind::Skill)
            .count();
        assert!(plugins > 0, "catalog must have plugin entries");
        assert!(skills > 0, "catalog must have skill entries");
    }
}
