//! Clawhub skill marketplace dispatch handlers.
//!
//! Clawhub (https://clawhub.ai) is a community skill registry.
//! API: GET /api/v1/skills (list), GET /api/v1/download?slug=X (download ZIP).

use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};
use std::path::PathBuf;

use super::err;
use crate::AppState;

const CLAWHUB_BASE: &str = "https://clawhub.ai";

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "clawhub.list" => Some(handle_clawhub_list(state, p).await),
        "clawhub.install" => Some(handle_clawhub_install(state, p).await),
        "clawhub.uninstall" => Some(handle_clawhub_uninstall(state, p).await),
        _ => None,
    }
}

// ── clawhub.list ─────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ClawhubSkillItem {
    slug: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    summary: Option<String>,
    tags: Option<serde_json::Value>,
    stats: Option<ClawhubStats>,
    #[serde(rename = "latestVersion")]
    latest_version: Option<ClawhubVersion>,
    #[serde(rename = "createdAt")]
    created_at: Option<i64>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<i64>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ClawhubStats {
    downloads: Option<i64>,
    stars: Option<i64>,
    versions: Option<i64>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ClawhubVersion {
    version: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: Option<i64>,
}

#[derive(serde::Deserialize)]
struct ClawhubListResponse {
    items: Vec<ClawhubSkillItem>,
    #[serde(rename = "nextCursor")]
    #[allow(dead_code)]
    next_cursor: Option<String>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ClawhubSearchResponse {
    results: Option<Vec<ClawhubSearchResult>>,
    items: Option<Vec<ClawhubSkillItem>>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ClawhubSearchResult {
    slug: String,
}

fn skills_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".loom")
        .join("skills")
}

fn cache_file() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".loom")
        .join("clawhub_skills_cache.json")
}

/// Cache TTL: 1 hour in seconds
const CACHE_TTL_SECS: u64 = 3600;

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    timestamp: u64,
    items: Vec<Value>,
}

fn is_installed(slug: &str) -> bool {
    skills_dir().join(slug).is_dir()
}

fn read_installed_version(slug: &str) -> Option<String> {
    let dir = skills_dir().join(slug);
    if !dir.is_dir() {
        return None;
    }
    let version_file = dir.join("version.txt");
    if let Ok(v) = std::fs::read_to_string(&version_file) {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// Read cached skill list if it exists and is fresh enough.
async fn read_cache() -> Option<Vec<Value>> {
    tokio::task::spawn_blocking(|| {
        let path = cache_file();
        let data = std::fs::read_to_string(&path).ok()?;
        let cache: CacheEntry = serde_json::from_str(&data).ok()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - cache.timestamp < CACHE_TTL_SECS {
            Some(cache.items)
        } else {
            None
        }
    })
    .await
    .unwrap_or(None)
}

/// Write skill list to cache file.
async fn write_cache(items: Vec<Value>) {
    tokio::task::spawn_blocking(move || {
        let path = cache_file();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cache = CacheEntry {
            timestamp: now,
            items,
        };
        if let Ok(json) = serde_json::to_string(&cache) {
            let _ = std::fs::write(&path, json);
        }
    })
    .await
    .unwrap_or(());
}

/// Enrich cached items with live installed status (runs filesystem checks on blocking pool).
async fn enrich_install_status(items: Vec<Value>) -> Vec<Value> {
    let fallback = items.clone();
    tokio::task::spawn_blocking(move || {
        items
            .into_iter()
            .map(|item| {
                let slug = item["id"].as_str().unwrap_or("");
                let installed = is_installed(slug);
                let installed_version = if installed {
                    read_installed_version(slug)
                } else {
                    None
                };
                let latest_ver = item["version"].as_str().unwrap_or("");
                let has_update = installed
                    && !latest_ver.is_empty()
                    && installed_version.as_deref() != Some(latest_ver);

                let mut enriched = item.clone();
                enriched["installed"] = json!(installed);
                enriched["installed_version"] = json!(installed_version);
                enriched["has_update"] = json!(has_update);
                enriched
            })
            .collect()
    })
    .await
    .unwrap_or(fallback)
}

async fn handle_clawhub_list(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let search = p.get("search").and_then(|v| v.as_str()).unwrap_or("");
    let force = p.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or(CLAWHUB_BASE);

    // Search results are not cached
    if !search.is_empty() {
        let skills = fetch_search_results(base_url, search).await?;
        let _ = state;
        return Ok(json!({ "skills": skills }));
    }

    // Check cache first (unless force refresh)
    if !force && let Some(cached) = read_cache().await {
        let enriched = enrich_install_status(cached).await;
        return Ok(json!({ "skills": enriched, "cached": true }));
    }

    // Fetch fresh data
    let skills = fetch_skill_list(base_url).await?;
    write_cache(skills.clone()).await;
    let enriched = enrich_install_status(skills).await;
    Ok(json!({ "skills": enriched, "cached": false }))
}

async fn build_http_client() -> Result<reqwest::Client, JsonRpcError> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))
}

async fn fetch_skill_list(base_url: &str) -> Result<Vec<Value>, JsonRpcError> {
    let client = build_http_client().await?;
    let list_url = format!("{}/api/v1/skills", base_url);

    let mut all_items: Vec<ClawhubSkillItem> = Vec::new();
    let mut cursor: Option<String> = None;

    // Paginate through all pages using the nextCursor field
    loop {
        let mut url = list_url.clone();
        if let Some(ref c) = cursor {
            url = format!("{}?cursor={}", url, urlencoding::encode(c));
        }

        let resp = client
            .get(&url)
            .header("User-Agent", "openLoom/0.2")
            .send()
            .await
            .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

        let body = resp
            .text()
            .await
            .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

        let list: ClawhubListResponse = serde_json::from_str(&body)
            .map_err(|e| err(ErrorCode::InternalError, &format!("parse error: {}", e)))?;

        all_items.extend(list.items);

        // Continue if there's a next page, otherwise break
        if list.next_cursor.is_some() && !list.next_cursor.as_deref().unwrap_or("").is_empty() {
            cursor = list.next_cursor;
        } else {
            break;
        }
    }

    let skills: Vec<Value> = all_items
        .into_iter()
        .map(|item| {
            let latest_ver = item
                .latest_version
                .as_ref()
                .and_then(|v| v.version.as_deref())
                .unwrap_or("");
            json!({
                "id": item.slug,
                "name": item.display_name.unwrap_or_else(|| item.slug.clone()),
                "description": item.summary.unwrap_or_default(),
                "version": latest_ver,
                "author": "",
                "category": "",
                "kind": "skill",
                "tags": item.tags.unwrap_or(serde_json::Value::Null),
                "downloads": item.stats.as_ref().and_then(|s| s.downloads).unwrap_or(0),
                "stars": item.stats.as_ref().and_then(|s| s.stars).unwrap_or(0),
                "source": "clawhub",
            })
        })
        .collect();

    Ok(skills)
}

async fn fetch_search_results(base_url: &str, search: &str) -> Result<Vec<Value>, JsonRpcError> {
    let client = build_http_client().await?;
    let url = format!(
        "{}/api/v1/search?q={}",
        base_url,
        urlencoding::encode(search)
    );

    let resp = client
        .get(&url)
        .header("User-Agent", "openLoom/0.2")
        .send()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    let body = resp
        .text()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    let items: Vec<ClawhubSkillItem> =
        if let Ok(search_resp) = serde_json::from_str::<ClawhubSearchResponse>(&body) {
            if let Some(results) = search_resp.results {
                let mut items = Vec::new();
                for r in results {
                    let detail_url = format!("{}/api/v1/skills/{}", base_url, r.slug);
                    if let Ok(detail_resp) = client
                        .get(&detail_url)
                        .header("User-Agent", "openLoom/0.2")
                        .send()
                        .await
                        && let Ok(detail_body) = detail_resp.text().await
                        && let Ok(detail) = serde_json::from_str::<Value>(&detail_body)
                        && let Some(skill) = detail.get("skill")
                        && let Ok(item) = serde_json::from_value::<ClawhubSkillItem>(skill.clone())
                    {
                        items.push(item);
                    }
                }
                items
            } else {
                search_resp.items.unwrap_or_default()
            }
        } else {
            Vec::new()
        };

    let skills: Vec<Value> = items
        .into_iter()
        .map(|item| {
            let installed = is_installed(&item.slug);
            let installed_version = if installed {
                read_installed_version(&item.slug)
            } else {
                None
            };
            let latest_ver = item
                .latest_version
                .as_ref()
                .and_then(|v| v.version.as_deref())
                .unwrap_or("");
            let has_update = installed
                && !latest_ver.is_empty()
                && installed_version.as_deref() != Some(latest_ver);

            json!({
                "id": item.slug,
                "name": item.display_name.unwrap_or_else(|| item.slug.clone()),
                "description": item.summary.unwrap_or_default(),
                "version": latest_ver,
                "author": "",
                "category": "",
                "kind": "skill",
                "tags": item.tags.unwrap_or(serde_json::Value::Null),
                "downloads": item.stats.as_ref().and_then(|s| s.downloads).unwrap_or(0),
                "stars": item.stats.as_ref().and_then(|s| s.stars).unwrap_or(0),
                "installed": installed,
                "installed_version": installed_version,
                "has_update": has_update,
                "source": "clawhub",
            })
        })
        .collect();

    Ok(skills)
}

// ── clawhub.install ──────────────────────────────────────────────────────

async fn handle_clawhub_install(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let slug = p
        .get("slug")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "slug required"))?;
    let base_url = p.get("base_url").and_then(|v| v.as_str()).unwrap_or(CLAWHUB_BASE);

    let download_url = format!("{}/api/v1/download?slug={}", base_url, slug);
    let target_dir = skills_dir().join(slug);

    // Create target directory
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| err(ErrorCode::InternalError, &format!("create dir: {}", e)))?;

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    let resp = client
        .get(&download_url)
        .header("User-Agent", "openLoom/0.2")
        .send()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    if !resp.status().is_success() {
        return Err(err(
            ErrorCode::InternalError,
            &format!("download failed: HTTP {}", resp.status()),
        ));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;

    // Extract ZIP
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| err(ErrorCode::InternalError, &format!("zip error: {}", e)))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| err(ErrorCode::InternalError, &format!("zip entry: {}", e)))?;
        let name = file.name().to_string();
        // Skip directory entries
        if name.ends_with('/') {
            continue;
        }
        // Reject zip-slip: only accept entry names built from normal / current-dir
        // components (no `..`, no absolute root, no drive prefix).
        let rel = std::path::Path::new(&name);
        if rel
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir))
        {
            continue;
        }
        let dest_path = target_dir.join(rel);
        // Defense in depth: the resolved path must stay under target_dir.
        if !dest_path.starts_with(&target_dir) {
            continue;
        }
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut out = std::fs::File::create(&dest_path)
            .map_err(|e| err(ErrorCode::InternalError, &format!("create file: {}", e)))?;
        std::io::copy(&mut file, &mut out)
            .map_err(|e| err(ErrorCode::InternalError, &format!("write file: {}", e)))?;
    }

    // Rename the first .md file (that looks like a skill) to SKILL.md if one doesn't exist
    if !target_dir.join("SKILL.md").exists()
        && let Ok(entries) = std::fs::read_dir(&target_dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                // Check if it looks like a skill (has frontmatter or skill-like content)
                if content.contains("---")
                    || content.contains("name:")
                    || content.contains("description:")
                {
                    let _ = std::fs::rename(&path, target_dir.join("SKILL.md"));
                    break;
                }
            }
        }
    }

    // Write version file
    let _ = std::fs::write(target_dir.join("version.txt"), slug);

    // Reload skills into orchestrator
    let _ = crate::dispatch::skills::reload_skills_into_orchestrator(&state.orchestrator).await;

    Ok(json!({ "ok": true, "slug": slug, "path": target_dir.to_string_lossy() }))
}

// ── clawhub.uninstall ────────────────────────────────────────────────────

async fn handle_clawhub_uninstall(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let slug = p
        .get("slug")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "slug required"))?;

    let target_dir = skills_dir().join(slug);

    if !target_dir.is_dir() {
        return Err(err(
            ErrorCode::InvalidRequest,
            &format!("skill '{}' not installed", slug),
        ));
    }

    std::fs::remove_dir_all(&target_dir)
        .map_err(|e| err(ErrorCode::InternalError, &format!("remove dir: {}", e)))?;

    // Reload skills into orchestrator
    let _ = crate::dispatch::skills::reload_skills_into_orchestrator(&state.orchestrator).await;

    Ok(json!({ "ok": true, "slug": slug }))
}
