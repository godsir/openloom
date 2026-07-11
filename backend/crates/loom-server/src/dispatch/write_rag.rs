//! RAG retrieval dispatch for Write mode.
//!
//! Provides BM25 keyword-based workspace indexing and search.
//! In-memory index with TTL-based cache invalidation.
//!
//! Methods: write.index_workspace, write.search_workspace, write.reindex_file

use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::err;
use crate::AppState;

// ============================================================
// In-memory index structures
// ============================================================

/// A single text chunk from a workspace file.
#[derive(Clone)]
struct TextChunk {
    file_path: String,
    text: String,
    char_start: usize,
    char_end: usize,
}

/// BM25-style inverted index over workspace files.
/// Lives in AppState behind an RwLock for concurrent access.
pub struct WorkspaceIndex {
    /// Chunks indexed, ordered by insertion.
    chunks: Vec<TextChunk>,
    /// Term → list of (chunk_index, term_frequency_in_chunk)
    inverted: HashMap<String, Vec<(usize, u32)>>,
    /// Total number of terms indexed (for BM25 average doc length).
    total_terms: usize,
    /// When this index was built (epoch millis).
    built_at: u64,
    /// TTL in millis (30s default).
    ttl_ms: u64,
    /// Max files to index.
    max_files: usize,
    /// Max bytes per file.
    max_file_bytes: usize,
    /// Chunk size in chars.
    chunk_size: usize,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            inverted: HashMap::new(),
            total_terms: 0,
            built_at: 0,
            ttl_ms: 30_000,
            max_files: 160,
            max_file_bytes: 600_000,
            chunk_size: 900,
        }
    }

    pub fn is_stale(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        now.saturating_sub(self.built_at) > self.ttl_ms
    }
}

// ============================================================
// Public handle function (dispatch entry point)
// ============================================================

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        "write.index_workspace" => Some(handle_index_workspace(state, p).await),
        "write.search_workspace" => Some(handle_search_workspace(state, p).await),
        "write.reindex_file" => Some(handle_reindex_file(state, p).await),
        _ => None,
    }
}

// ============================================================
// RPC implementations
// ============================================================

async fn handle_index_workspace(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let workspace_root = p
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if workspace_root.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "workspace_root required"));
    }

    let ws_path = PathBuf::from(workspace_root)
        .canonicalize()
        .map_err(|_| err(ErrorCode::PermissionDenied, "invalid workspace_root"))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Build index by scanning workspace
    let mut index = WorkspaceIndex::new();
    index.built_at = now;

    match scan_and_index(&ws_path, &mut index, "", 0) {
        Ok(count) => {
            // Store index in AppState
            let chunks_len = index.chunks.len();
            if let Ok(mut guard) = state.write_index.write() {
                *guard = Some(index);
            }
            Ok(json!({
                "ok": true,
                "files_indexed": count,
                "total_chunks": chunks_len,
            }))
        }
        Err(e) => Err(err(ErrorCode::InternalError, &e)),
    }
}

async fn handle_search_workspace(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let query = p.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let top_k = p.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    if query.is_empty() {
        return Ok(json!({ "ok": true, "results": [] }));
    }

    // Get index from state
    let guard = state.write_index.read().unwrap();
    let index = match guard.as_ref() {
        Some(idx) => idx,
        None => {
            return Ok(json!({
                "ok": false,
                "error": "no index built — call write.index_workspace first",
                "results": []
            }));
        }
    };

    // Check staleness
    if index.is_stale() {
        drop(guard);
        return Ok(json!({
            "ok": false,
            "error": "index is stale, please re-index",
            "results": []
        }));
    }

    // Tokenize query
    let query_terms: Vec<String> = tokenize(query);
    if query_terms.is_empty() {
        return Ok(json!({ "ok": true, "results": [] }));
    }

    // BM25 scoring
    let n = index.chunks.len() as f64;
    let avgdl = if n > 0.0 {
        index.total_terms as f64 / n
    } else {
        1.0
    };
    let k1 = 1.2;
    let b = 0.75;

    let mut scores: Vec<(usize, f64)> = Vec::new();

    for term in &query_terms {
        if let Some(postings) = index.inverted.get(term) {
            let df = postings.len() as f64;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

            for &(chunk_idx, tf) in postings {
                if chunk_idx >= index.chunks.len() {
                    continue;
                }
                let doc_len = index.chunks[chunk_idx].text.len() as f64;
                let tf_norm =
                    (tf as f64 * (k1 + 1.0)) / (tf as f64 + k1 * (1.0 - b + b * doc_len / avgdl));
                let score = idf * tf_norm;

                match scores.iter().position(|&(i, _)| i == chunk_idx) {
                    Some(pos) => scores[pos].1 += score,
                    None => scores.push((chunk_idx, score)),
                }
            }
        }
    }

    // Sort by score desc, take top-K
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scores.truncate(top_k);

    let results: Vec<Value> = scores
        .into_iter()
        .filter(|&(_, score)| score > 0.0)
        .map(|(chunk_idx, score)| {
            let chunk = &index.chunks[chunk_idx];
            json!({
                "file_path": chunk.file_path,
                "text": chunk.text,
                "char_start": chunk.char_start,
                "char_end": chunk.char_end,
                "score": score,
            })
        })
        .collect();

    Ok(json!({ "ok": true, "results": results }))
}

async fn handle_reindex_file(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let workspace_root = p
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let file_path = p.get("path").and_then(|v| v.as_str()).unwrap_or("");

    if workspace_root.is_empty() || file_path.is_empty() {
        return Err(err(
            ErrorCode::InvalidRequest,
            "workspace_root and path required",
        ));
    }

    let ws_path = PathBuf::from(workspace_root)
        .canonicalize()
        .map_err(|_| err(ErrorCode::PermissionDenied, "invalid workspace_root"))?;

    let full_path = ws_path.join(file_path);

    // Validate path is within workspace
    if let Ok(canonical) = full_path.canonicalize() {
        if !canonical.starts_with(&ws_path) {
            return Err(err(ErrorCode::PermissionDenied, "path escapes workspace"));
        }
    }

    // Remove old chunks for this file
    if let Ok(mut guard) = state.write_index.write() {
        if let Some(ref mut index) = *guard {
            // Remove from inverted index (simplified: rebuild without old chunks)
            let old_count = index.chunks.len();
            index.chunks.retain(|c| c.file_path != file_path);
            if index.chunks.len() != old_count {
                // Rebuild inverted index from scratch (simplified approach)
                rebuild_inverted(index);
            }

            // Read file and re-index
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let limited: String = content.chars().take(index.max_file_bytes).collect();
                let chunks = chunk_text(&limited, index.chunk_size);
                let base_offset = index.chunks.len();
                for (i, chunk_text) in chunks.iter().enumerate() {
                    index.chunks.push(TextChunk {
                        file_path: file_path.to_string(),
                        text: chunk_text.clone(),
                        char_start: i * index.chunk_size,
                        char_end: (i + 1) * index.chunk_size,
                    });
                    add_to_inverted(index, base_offset + i, chunk_text);
                }
            }

            index.built_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
        }
    }

    Ok(json!({ "ok": true }))
}

// ============================================================
// Indexing helpers
// ============================================================

fn scan_and_index(
    dir: &Path,
    index: &mut WorkspaceIndex,
    relative_prefix: &str,
    file_count: usize,
) -> Result<usize, String> {
    if file_count >= index.max_files {
        return Ok(file_count);
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(file_count),
    };

    let mut count = file_count;

    for entry in entries.flatten() {
        if count >= index.max_files {
            break;
        }

        let name = entry.file_name().to_string_lossy().to_string();

        // Skip dotfiles
        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let rel_path = if relative_prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", relative_prefix, name)
        };

        if path.is_dir() {
            count = scan_and_index(&path, index, &rel_path, count)?;
        } else if path.is_file() {
            // Only index text files
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext != "md" && ext != "txt" && ext != "markdown" {
                continue;
            }

            // Read file
            if let Ok(content) = std::fs::read_to_string(&path) {
                let limited: String = content.chars().take(index.max_file_bytes).collect();
                let chunks = chunk_text(&limited, index.chunk_size);
                let base_offset = index.chunks.len();
                for (i, chunk_text) in chunks.iter().enumerate() {
                    index.chunks.push(TextChunk {
                        file_path: rel_path.clone(),
                        text: chunk_text.clone(),
                        char_start: i * index.chunk_size,
                        char_end: (i + 1) * index.chunk_size,
                    });
                    add_to_inverted(index, base_offset + i, chunk_text);
                }
                count += 1;
            }
        }
    }

    Ok(count)
}

fn chunk_text(text: &str, chunk_size: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + chunk_size).min(chars.len());
        // Try to find a sentence/paragraph break near the end
        let mut actual_end = end;
        if end < chars.len() {
            let lookback = (end.saturating_sub(100)).max(start);
            for i in (lookback..end).rev() {
                let c = chars[i];
                if c == '\n'
                    || c == '。'
                    || c == '.'
                    || c == '！'
                    || c == '!'
                    || c == '？'
                    || c == '?'
                {
                    actual_end = i + 1;
                    break;
                }
            }
        }
        let chunk: String = chars[start..actual_end].iter().collect();
        if !chunk.trim().is_empty() {
            chunks.push(chunk);
        }
        start = actual_end;
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }

    chunks
}

fn tokenize(text: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    let lower = text.to_lowercase();

    // Split on word boundaries — support both English and CJK
    let mut current = String::new();
    for c in lower.chars() {
        if c.is_alphanumeric() {
            current.push(c);
        } else {
            if current.len() >= 2 {
                terms.push(current.clone());
            }
            current.clear();
            // Single CJK chars as bigrams
            if c as u32 > 0x4E00 {
                terms.push(c.to_string());
            }
        }
    }
    if current.len() >= 2 {
        terms.push(current);
    }

    terms
}

fn add_to_inverted(index: &mut WorkspaceIndex, chunk_idx: usize, text: &str) {
    let terms = tokenize(text);
    let mut term_counts: HashMap<String, u32> = HashMap::new();
    for term in terms {
        *term_counts.entry(term).or_insert(0) += 1;
        index.total_terms += 1;
    }
    for (term, tf) in term_counts {
        index
            .inverted
            .entry(term)
            .or_insert_with(Vec::new)
            .push((chunk_idx, tf));
    }
}

fn rebuild_inverted(index: &mut WorkspaceIndex) {
    index.inverted.clear();
    index.total_terms = 0;
    let items: Vec<(usize, String)> = index
        .chunks
        .iter()
        .enumerate()
        .map(|(i, c)| (i, c.text.clone()))
        .collect();
    for (i, text) in items {
        add_to_inverted(index, i, &text);
    }
}
