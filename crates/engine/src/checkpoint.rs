use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SKIP_EXTENSIONS: &[&str] = &[
    "7z", "apk", "app", "avi", "bin", "bmp", "bz2", "dmg", "doc", "docx", "ear", "elf", "exe",
    "gif", "gz", "ico", "iso", "jar", "jpeg", "jpg", "m4a", "mov", "mp3", "mp4", "mpeg", "mpg",
    "o", "obj", "odp", "ods", "odt", "otf", "pdf", "pkg", "png", "rar", "rpm", "so", "svg", "tar",
    "ttf", "war", "webp", "wmv", "woff", "woff2", "xz", "zip",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMeta {
    pub id: String,
    pub ts: u64,
    pub tool: String,
    pub path: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointEntry {
    pub ts: u64,
    pub tool: String,
    pub path: String,
    pub size: u64,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckpointRestore {
    pub path: String,
    pub content: String,
}

pub struct CheckpointStore {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl CheckpointStore {
    pub fn new(data_dir: &Path) -> Self {
        let dir = data_dir.join("checkpoints");
        let _ = std::fs::create_dir_all(&dir);
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    pub fn save(
        &self,
        file_path: &str,
        tool: &str,
        max_size_kb: u64,
        session_path: Option<&str>,
        reason: Option<&str>,
    ) -> Option<String> {
        // Skip binary files by extension
        if let Some(ext) = Path::new(file_path).extension().and_then(|e| e.to_str()) {
            if SKIP_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                return None;
            }
        }

        // Check file size
        let metadata = match std::fs::metadata(file_path) {
            Ok(m) => m,
            Err(_) => return None,
        };
        let size_bytes = metadata.len();
        if size_bytes > max_size_kb * 1024 {
            return None;
        }

        // Read content as UTF-8
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return None,
        };
        // Skip binary content (contains null bytes)
        if content.contains('\0') {
            return None;
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let id = format!("{}_{}", ts, &uuid::Uuid::new_v4().to_string()[..8]);

        let entry = CheckpointEntry {
            ts,
            tool: tool.to_string(),
            path: file_path.to_string(),
            size: size_bytes,
            content,
            session_path: session_path.map(|s| s.to_string()),
            reason: reason.map(|s| s.to_string()),
        };

        let _guard = self.lock.lock().unwrap();
        let file_path = self.dir.join(format!("{}.json", id));
        if let Ok(json) = serde_json::to_string(&entry) {
            if std::fs::write(&file_path, json).is_ok() {
                return Some(id);
            }
        }
        None
    }

    pub fn list(&self) -> Vec<CheckpointMeta> {
        let _guard = self.lock.lock().unwrap();
        let mut entries = Vec::new();

        let dir = match std::fs::read_dir(&self.dir) {
            Ok(d) => d,
            Err(_) => return entries,
        };

        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let id = stem.to_string();

            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(meta) = serde_json::from_str::<CheckpointEntry>(&content) {
                    entries.push(CheckpointMeta {
                        id,
                        ts: meta.ts,
                        tool: meta.tool,
                        path: meta.path,
                        size: meta.size,
                        session_path: meta.session_path,
                        reason: meta.reason,
                    });
                }
            }
        }

        entries.sort_by(|a, b| b.ts.cmp(&a.ts));
        entries
    }

    pub fn restore(&self, id: &str) -> Result<CheckpointRestore, String> {
        let _guard = self.lock.lock().unwrap();
        let file_path = self.dir.join(format!("{}.json", id));
        let content =
            std::fs::read_to_string(&file_path).map_err(|e| format!("read failed: {}", e))?;
        let entry: CheckpointEntry =
            serde_json::from_str(&content).map_err(|e| format!("parse failed: {}", e))?;

        // Create parent directories and write content back
        let target = Path::new(&entry.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir failed: {}", e))?;
        }
        std::fs::write(&entry.path, &entry.content).map_err(|e| format!("write failed: {}", e))?;

        Ok(CheckpointRestore {
            path: entry.path,
            content: entry.content,
        })
    }

    pub fn delete(&self, id: &str) -> bool {
        let _guard = self.lock.lock().unwrap();
        let file_path = self.dir.join(format!("{}.json", id));
        std::fs::remove_file(file_path).is_ok()
    }

    pub fn cleanup(&self, retention_days: u32) -> u64 {
        let _guard = self.lock.lock().unwrap();
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
            - (retention_days as u64 * 86_400_000);

        let mut deleted = 0u64;
        let dir = match std::fs::read_dir(&self.dir) {
            Ok(d) => d,
            Err(_) => return 0,
        };

        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(meta) = serde_json::from_str::<CheckpointEntry>(&content) {
                    if meta.ts < cutoff {
                        if std::fs::remove_file(&path).is_ok() {
                            deleted += 1;
                        }
                    }
                }
            }
        }
        deleted
    }
}
