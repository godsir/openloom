// Skill bundle management for openLoom.
//
// Bundles are stored as JSON in <data_dir>/skill-bundles.json.
// Supports CRUD, reorder, export (zip), and per-agent toggle state.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBundle {
    pub id: String,
    pub name: String,
    pub skill_names: Vec<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub source_package: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Per-agent enabled state: agent_id -> enabled
    #[serde(default)]
    pub enabled: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleStore {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    bundles: Vec<SkillBundle>,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn random_suffix() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let n: u32 = rng.random();
    format!("{:06x}", n)
}

fn slugify(value: &str) -> String {
    let s = value
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = s.trim_matches('-');
    if trimmed.is_empty() || trimmed.len() > 56 {
        trimmed.chars().take(56).collect()
    } else {
        trimmed.to_string()
    }
}

// ── Store load/save ──

fn bundles_path(data_dir: &Path) -> PathBuf {
    data_dir.join("skill-bundles.json")
}

fn load_store(data_dir: &Path) -> BundleStore {
    let path = bundles_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(BundleStore {
            schema_version: SCHEMA_VERSION,
            bundles: vec![],
        }),
        Err(_) => BundleStore {
            schema_version: SCHEMA_VERSION,
            bundles: vec![],
        },
    }
}

fn save_store(data_dir: &Path, store: &BundleStore) -> Result<()> {
    let path = bundles_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = format!("{}.{}.tmp", path.display(), std::process::id());
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

// ── Public API ──

pub fn list_bundles(data_dir: &Path) -> Vec<SkillBundle> {
    load_store(data_dir).bundles
}

pub fn create_bundle(data_dir: &Path, name: &str, skill_names: Vec<String>) -> Result<SkillBundle> {
    let mut store = load_store(data_dir);
    let id = format!("{}-{}", slugify(name), random_suffix());
    let now = now_iso();
    let bundle = SkillBundle {
        id,
        name: name.to_string(),
        skill_names,
        source: "user".to_string(),
        agent_id: None,
        source_package: None,
        created_at: now.clone(),
        updated_at: now,
        enabled: HashMap::new(),
    };
    store.bundles.push(bundle.clone());
    save_store(data_dir, &store)?;
    Ok(bundle)
}

pub fn update_bundle(
    data_dir: &Path,
    id: &str,
    name: Option<&str>,
    skill_names: Option<Vec<String>>,
) -> Result<SkillBundle> {
    let mut store = load_store(data_dir);
    let idx = store
        .bundles
        .iter()
        .position(|b| b.id == id)
        .context("bundle not found")?;
    let mut bundle = store.bundles[idx].clone();
    if let Some(n) = name {
        bundle.name = n.to_string();
    }
    if let Some(sn) = skill_names {
        bundle.skill_names = sn;
    }
    bundle.updated_at = now_iso();
    store.bundles[idx] = bundle.clone();
    save_store(data_dir, &store)?;
    Ok(bundle)
}

pub fn delete_bundle(data_dir: &Path, id: &str) -> Result<bool> {
    let mut store = load_store(data_dir);
    let before = store.bundles.len();
    store.bundles.retain(|b| b.id != id);
    if store.bundles.len() == before {
        return Ok(false);
    }
    save_store(data_dir, &store)?;
    Ok(true)
}

pub fn reorder_bundles(data_dir: &Path, bundle_ids: &[String]) -> Result<Vec<SkillBundle>> {
    let mut store = load_store(data_dir);
    if bundle_ids.len() != store.bundles.len() {
        anyhow::bail!("bundle_ids must include every bundle exactly once");
    }
    let mut by_id: HashMap<&str, SkillBundle> = store
        .bundles
        .iter()
        .map(|b| (b.id.as_str(), b.clone()))
        .collect();
    let now = now_iso();
    let ordered: Result<Vec<SkillBundle>> = bundle_ids
        .iter()
        .map(|id| {
            let mut b = by_id.remove(id.as_str()).context("unknown bundle id")?;
            b.updated_at = now.clone();
            Ok(b)
        })
        .collect();
    let ordered = ordered?;
    store.bundles = ordered.clone();
    save_store(data_dir, &store)?;
    Ok(ordered)
}

pub fn set_bundle_enabled(data_dir: &Path, id: &str, agent_id: &str, enabled: bool) -> Result<()> {
    let mut store = load_store(data_dir);
    let idx = store
        .bundles
        .iter()
        .position(|b| b.id == id)
        .context("bundle not found")?;
    store.bundles[idx]
        .enabled
        .insert(agent_id.to_string(), enabled);
    store.bundles[idx].updated_at = now_iso();
    save_store(data_dir, &store)?;
    Ok(())
}

pub fn get_bundle_enabled(data_dir: &Path, id: &str, agent_id: &str) -> bool {
    let store = load_store(data_dir);
    store
        .bundles
        .iter()
        .find(|b| b.id == id)
        .and_then(|b| b.enabled.get(agent_id))
        .copied()
        .unwrap_or(false)
}

pub fn export_bundle(data_dir: &Path, skills_dir: &Path, id: &str) -> Result<ExportResult> {
    let store = load_store(data_dir);
    let bundle = store
        .bundles
        .iter()
        .find(|b| b.id == id)
        .context("bundle not found")?
        .clone();

    let mut warnings: Vec<ExportWarning> = Vec::new();
    let mut exported_skills: Vec<ExportedSkill> = Vec::new();

    // Build temp export directory
    let token = format!("{}-{}", std::process::id(), random_suffix());
    let tmp_root = data_dir
        .join(".ephemeral")
        .join("skill-bundle-exports")
        .join(&token);
    let pkg_root = tmp_root.join("package");
    let pkg_skills = pkg_root.join("skills");
    std::fs::create_dir_all(&pkg_skills)?;

    for skill_name in &bundle.skill_names {
        let source_dir = skills_dir.join(skill_name);
        if !source_dir.exists() || !source_dir.join("SKILL.md").exists() {
            warnings.push(ExportWarning {
                r#type: "missing-skill".to_string(),
                name: skill_name.clone(),
            });
            continue;
        }
        let safe_name = sanitize_name(skill_name);
        let dest = pkg_skills.join(&safe_name);
        copy_dir_recursive(&source_dir, &dest)?;
        exported_skills.push(ExportedSkill {
            name: safe_name.clone(),
            path: format!("skills/{}", safe_name),
        });
    }

    if exported_skills.is_empty() {
        let _ = std::fs::remove_dir_all(&tmp_root);
        anyhow::bail!("bundle has no exportable skills");
    }

    // Write bundle manifest
    let manifest = serde_json::json!({
        "kind": "SkillBundle",
        "schemaVersion": SCHEMA_VERSION,
        "package": {
            "name": format!("{}-skillbundle.zip", slugify(&bundle.name)),
            "exportedAt": now_iso(),
        },
        "bundle": {
            "name": bundle.name,
            "source": bundle.source,
            "sourcePackage": bundle.source_package,
        },
        "skills": {
            "bundles": [{
                "name": bundle.name,
                "skills": exported_skills.iter().map(|s| serde_json::json!({
                    "name": s.name,
                    "path": s.path,
                })).collect::<Vec<_>>(),
            }],
        },
    });
    std::fs::write(
        pkg_root.join("bundle.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;

    // Create zip
    let file_name = format!("{}-skillbundle.zip", slugify(&bundle.name));
    let file_path = data_dir.join(&file_name);
    let zip_file = std::fs::File::create(&file_path)?;
    let mut zip_writer = zip::ZipWriter::new(zip_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip_writer, &pkg_root, "", &options)?;

    // Cleanup temp
    let _ = std::fs::remove_dir_all(&tmp_root);

    Ok(ExportResult {
        file_path: file_path.to_string_lossy().to_string(),
        file_name,
        bundle: ExportedBundle {
            id: bundle.id,
            name: bundle.name,
            skill_count: exported_skills.len(),
        },
        warnings,
    })
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir: &Path,
    prefix: &str,
    options: &zip::write::SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = if prefix.is_empty() {
            entry.file_name().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, entry.file_name().to_string_lossy())
        };

        if path.is_dir() {
            zip.add_directory(&name, *options)?;
            add_dir_to_zip(zip, &path, &name, options)?;
        } else {
            zip.start_file(&name, *options)?;
            let data = std::fs::read(&path)?;
            zip.write_all(&data)?;
        }
    }
    Ok(())
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub file_path: String,
    pub file_name: String,
    pub bundle: ExportedBundle,
    pub warnings: Vec<ExportWarning>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedBundle {
    pub id: String,
    pub name: String,
    pub skill_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportWarning {
    pub r#type: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedSkill {
    pub name: String,
    pub path: String,
}
