use std::fs;

use openloom_skills::Skill;
use openloom_skills::external::ExternalSkill;
use openloom_skills::loom_context::LoomContext;
use openloom_skills::plugin_loader::PluginLoader;

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

    let skill =
        ExternalSkill::from_skill_md(content, "test-plugin").expect("Should parse valid SKILL.md");
    assert_eq!(skill.qualified_name(), "test-plugin:test-skill");
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

    let skills = PluginLoader::discover(data_dir.path(), cwd.path());
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

    let context = LoomContext::load(data_dir.path(), cwd.path());
    assert!(context.contains("Always use TDD"));
    assert!(context.contains("Prefer Rust"));
}
