//! KG dispatch handlers — kg.* / cognitions.* / stats.*

use loom_types::{ErrorCode, JsonRpcError};
use serde_json::{Value, json};

use super::err;
use crate::AppState;

pub async fn handle(
    state: &AppState,
    method: &str,
    p: &Value,
) -> Option<Result<Value, JsonRpcError>> {
    match method {
        // KG
        "kg.search" => Some(handle_kg_search(state, p).await),
        "kg.stats" => Some(handle_kg_stats(state).await),
        "kg.neighbors" => Some(handle_kg_neighbors(state, p).await),
        "kg.walk" => Some(handle_kg_walk(state, p).await),
        "kg.list" => Some(handle_kg_list(state, p).await),
        "kg.edges_between" => Some(handle_kg_edges_between(state, p).await),
        "kg.node.delete" => Some(handle_kg_node_delete(state, p).await),
        "kg.edge.delete" => Some(handle_kg_edge_delete(state, p).await),
        "kg.prune" => Some(handle_kg_prune(state, p).await),
        // Cognitions
        "cognitions.list" => Some(handle_cognitions_list(state, p).await),
        "cognitions.snapshots" => Some(handle_cognitions_snapshots(state, p).await),
        "cognitions.subjects" => Some(handle_cognitions_subjects(state).await),
        "cognitions.delete" => Some(handle_cognitions_delete(state, p).await),
        // Token usage stats
        "stats.token_summary" => Some(handle_stats_token_summary(state, p).await),
        "stats.token_history" => Some(handle_stats_token_history(state, p).await),
        "stats.reset" => Some(handle_stats_reset(state).await),
        // Memory
        "memory.promote" => Some(handle_memory_promote(state, p).await),
        "memory.quality" => Some(handle_memory_quality(state, p).await),
        "memory.health" => Some(handle_memory_health(state).await),
        "memory.persona" => Some(handle_memory_persona(state).await),
        "memory.patterns" => Some(handle_memory_patterns(state).await),
        "memory.consolidate" => Some(handle_memory_consolidate(state).await),
        "memory.forget" => Some(handle_memory_forget(state, p).await),
        "memory.promote_to_layer" => Some(handle_memory_promote_to_layer(state, p).await),
        "memory.pipeline_status" => Some(handle_memory_pipeline_status(state).await),
        "memory.layer_stats" => Some(handle_memory_layer_stats(state).await),
        _ => None,
    }
}

// --- kg.search ---

async fn handle_kg_search(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let query = p.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let rows = state
        .orchestrator
        .kg_search(query, limit)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "rows": rows }))
}

// --- kg.stats ---

async fn handle_kg_stats(state: &AppState) -> Result<Value, JsonRpcError> {
    let stats = state
        .orchestrator
        .kg_stats()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(stats).unwrap_or_default())
}

// --- kg.neighbors ---

async fn handle_kg_neighbors(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let node_name = p.get("node_name").and_then(|v| v.as_str()).unwrap_or("");
    if node_name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "node_name required"));
    }
    let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as usize;
    let scope = p.get("scope").and_then(|v| v.as_str());
    let graph = state
        .orchestrator
        .kg_neighbors(node_name, limit, scope)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(graph).unwrap_or_default())
}

// --- kg.walk ---

async fn handle_kg_walk(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let start_name = p.get("start_name").and_then(|v| v.as_str()).unwrap_or("");
    if start_name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "start_name required"));
    }
    let max_depth = p.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(2) as u8;
    let scope = p.get("scope").and_then(|v| v.as_str());
    let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let graph = state
        .orchestrator
        .kg_walk(start_name, max_depth, scope, limit)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(graph).unwrap_or_default())
}

// --- kg.list ---

async fn handle_kg_list(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = p.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let scope = p.get("scope").and_then(|v| v.as_str());
    let nodes = state
        .orchestrator
        .kg_list_nodes(limit, offset, scope)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "nodes": nodes }))
}

// --- kg.edges_between ---

async fn handle_kg_edges_between(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let node_names: Vec<String> = p
        .get("node_names")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let scope = p.get("scope").and_then(|v| v.as_str());
    let edges = state
        .orchestrator
        .kg_edges_between(&node_names, scope)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "edges": edges }))
}

// --- kg.node.delete ---

async fn handle_kg_node_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "name required"));
    }
    let deleted = state
        .orchestrator
        .kg_delete_node(name)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "deleted": deleted }))
}

// --- kg.edge.delete ---

async fn handle_kg_edge_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let source = p.get("source").and_then(|v| v.as_str()).unwrap_or("");
    let target = p.get("target").and_then(|v| v.as_str()).unwrap_or("");
    let relation = p.get("relation").and_then(|v| v.as_str()).unwrap_or("");
    if source.is_empty() || target.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "source and target required"));
    }
    let deleted = state
        .orchestrator
        .kg_delete_edge(source, target, relation)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "deleted": deleted }))
}

// --- kg.prune ---

async fn handle_kg_prune(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let older_than_days = p
        .get("older_than_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(30);
    let pruned_count = state
        .orchestrator
        .kg_prune(older_than_days)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "pruned_count": pruned_count }))
}

// --- cognitions.list ---

async fn handle_cognitions_list(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let subject = p.get("subject").and_then(|v| v.as_str()).unwrap_or("USER");
    let scope = p.get("scope").and_then(|v| v.as_str());
    let limit = p.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = p.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let rows = state
        .orchestrator
        .cognition_list(subject, scope, limit, offset)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "rows": rows }))
}

// --- cognitions.snapshots ---

async fn handle_cognitions_snapshots(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let cognition_id = p.get("cognition_id").and_then(|v| v.as_i64()).unwrap_or(0);
    if cognition_id == 0 {
        return Err(err(ErrorCode::InvalidRequest, "cognition_id required"));
    }
    let snapshots = state
        .orchestrator
        .cognition_snapshots(cognition_id)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "snapshots": snapshots }))
}

// --- cognitions.subjects ---

async fn handle_cognitions_subjects(state: &AppState) -> Result<Value, JsonRpcError> {
    let subjects = state
        .orchestrator
        .cognition_list_subjects()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "subjects": subjects }))
}

// --- cognitions.delete ---

async fn handle_cognitions_delete(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let id = p
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "id (integer) required"))?;
    let deleted = state
        .orchestrator
        .cognition_delete(id)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "deleted": deleted }))
}

// --- stats.token_summary ---

async fn handle_stats_token_summary(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let from = p
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("1970-01-01");
    let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
    let summary = state
        .orchestrator
        .token_summary(from, to)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(summary)
}

// --- stats.token_history ---

async fn handle_stats_token_history(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let from = p
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("1970-01-01");
    let to = p.get("to").and_then(|v| v.as_str()).unwrap_or("2099-12-31");
    let granularity = p
        .get("granularity")
        .and_then(|v| v.as_str())
        .unwrap_or("day");
    let history = state
        .orchestrator
        .token_history(from, to, granularity)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(history)
}

// --- stats.reset ---

async fn handle_stats_reset(state: &AppState) -> Result<Value, JsonRpcError> {
    state
        .orchestrator
        .reset_token_usage()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}

// --- memory.promote ---

async fn handle_memory_promote(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let session_id = p.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
    if session_id.is_empty() {
        return Err(err(ErrorCode::InvalidRequest, "session_id required"));
    }
    let min_confidence = p
        .get("min_confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);
    let node_names: Vec<String> = p
        .get("node_names")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let cognition_ids: Vec<i64> = p
        .get("cognition_ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();
    let (promoted_nodes, promoted_cognitions) = state
        .orchestrator
        .memory_promote(session_id, min_confidence, &node_names, &cognition_ids)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({
        "promoted_nodes": promoted_nodes,
        "promoted_cognitions": promoted_cognitions
    }))
}

// --- memory.quality ---

async fn handle_memory_quality(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let lookback_days = p
        .get("lookback_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(30);
    let report = state
        .orchestrator
        .memory_quality_report(lookback_days)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(serde_json::to_value(report).unwrap_or_default())
}

// --- memory.health ---

async fn handle_memory_health(state: &AppState) -> Result<Value, JsonRpcError> {
    let health = state
        .orchestrator
        .get_memory_health()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    serde_json::from_str::<Value>(&health)
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))
}

// --- memory.persona ---

async fn handle_memory_persona(state: &AppState) -> Result<Value, JsonRpcError> {
    let persona_text = state
        .orchestrator
        .get_rich_persona()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    let persona_structured = state
        .orchestrator
        .get_rich_persona_structured()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "persona": persona_text, "rich_persona": persona_structured }))
}

// --- memory.patterns ---

async fn handle_memory_patterns(state: &AppState) -> Result<Value, JsonRpcError> {
    let patterns = state
        .orchestrator
        .detect_patterns()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    serde_json::from_str::<Value>(&patterns)
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))
}

// --- memory.consolidate ---

async fn handle_memory_consolidate(state: &AppState) -> Result<Value, JsonRpcError> {
    let report = state
        .orchestrator
        .run_consolidation_cycle()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    serde_json::from_str::<Value>(&report)
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))
}

// --- memory.forget ---

async fn handle_memory_forget(state: &AppState, p: &Value) -> Result<Value, JsonRpcError> {
    let min_importance = p
        .get("min_importance")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3);
    let max_age_days = p.get("max_age_days").and_then(|v| v.as_i64()).unwrap_or(90);
    let report = state
        .orchestrator
        .run_forgetting_cycle(min_importance, max_age_days)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    serde_json::from_str::<Value>(&report)
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))
}

// --- memory.pipeline_status ---

async fn handle_memory_pipeline_status(state: &AppState) -> Result<Value, JsonRpcError> {
    let status = state
        .orchestrator
        .get_pipeline_status()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    serde_json::from_str::<Value>(&status)
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))
}

// --- memory.layer_stats ---

async fn handle_memory_layer_stats(state: &AppState) -> Result<Value, JsonRpcError> {
    let stats = state
        .orchestrator
        .get_layer_stats()
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "layers": stats }))
}

// --- memory.promote_to_layer ---

async fn handle_memory_promote_to_layer(
    state: &AppState,
    p: &Value,
) -> Result<Value, JsonRpcError> {
    let node_name = p
        .get("node_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "missing node_name"))?;
    let target_layer = p
        .get("target_layer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err(ErrorCode::InvalidRequest, "missing target_layer"))?;
    state
        .orchestrator
        .promote_to_layer(node_name, target_layer)
        .await
        .map_err(|e| err(ErrorCode::InternalError, &e.to_string()))?;
    Ok(json!({ "ok": true }))
}
