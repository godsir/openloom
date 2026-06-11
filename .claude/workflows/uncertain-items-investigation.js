export const meta = {
  name: 'uncertain-items-investigation',
  description: 'Deep investigation of 7 uncertain audit items: call paths, perf risks, hidden usage, git history',
  phases: [
    { title: 'Deep Trace', detail: 'Trace 7 uncertain items through code paths, benchmarks, git history, dynamic imports' },
    { title: 'Neutral Review', detail: 'Independent neutral reviewers examine each finding without bias' },
    { title: 'Adversarial Panel', detail: '3 skeptics try to refute deep-trace conclusions from opposite angles' },
    { title: 'Final Synthesis', detail: 'Merge all analyses into definitive conclusions with confidence levels' }
  ]
}

// ─── Shared schemas ───

var S1 = { type: 'object', properties: { function_names: { type: 'array', items: { type: 'string' } }, call_chain: { type: 'string' }, reachable_in_serve: { type: 'boolean' }, reachable_via_jsonrpc: { type: 'boolean' }, frontend_invokes: { type: 'boolean' }, actual_column_name: { type: 'string' }, crash_confirmed: { type: 'boolean' }, confidence: { type: 'string' }, evidence: { type: 'string' } } }

var S2 = { type: 'object', properties: { query_patterns: { type: 'array', items: { type: 'object' } }, existing_indexes: { type: 'array', items: { type: 'string' } }, estimated_max_rows: { type: 'number' }, caching_mitigation: { type: 'string' }, sqlite_would_use_partial_scan: { type: 'boolean' }, user_noticeable: { type: 'boolean' }, recommended: { type: 'boolean' }, confidence: { type: 'string' }, evidence: { type: 'string' } } }

var S3 = { type: 'object', properties: { tiptap_packages: { type: 'array', items: { type: 'string' } }, installed_on_disk: { type: 'boolean' }, import_count: { type: 'number' }, dynamic_import_found: { type: 'boolean' }, vite_config_reference: { type: 'boolean' }, git_added_date: { type: 'string' }, planned_usage_evidence: { type: 'string' }, conclusion: { type: 'string' }, confidence: { type: 'string' }, evidence: { type: 'string' } } }

var S4 = { type: 'object', properties: { v8_changes_summary: { type: 'string' }, v1_comparison: { type: 'string' }, production_code_references_v8_columns: { type: 'boolean' }, test_only_assertions: { type: 'array', items: { type: 'string' } }, git_commit_history: { type: 'string' }, developer_intent: { type: 'string' }, safe_to_apply: { type: 'boolean' }, recommended_action: { type: 'string' }, confidence: { type: 'string' }, evidence: { type: 'string' } } }

var S5 = { type: 'object', properties: { field_status: { type: 'array', items: { type: 'object' } }, total_parsed_not_consumed: { type: 'number' }, total_not_even_parsed: { type: 'number' }, git_activity_level: { type: 'string' }, roadmap_reference: { type: 'string' }, recommendation: { type: 'string' }, confidence: { type: 'string' }, evidence: { type: 'string' } } }

var S6 = { type: 'object', properties: { prune_stale_order: { type: 'string' }, consolidation_order: { type: 'string' }, stale_node_definition: { type: 'string' }, stale_nodes_typically_have_edges: { type: 'boolean' }, foreign_keys_enabled: { type: 'boolean' }, fk_constraints_exist_in_v1: { type: 'boolean' }, crash_reachable: { type: 'boolean' }, silent_failure_reachable: { type: 'boolean' }, observed_errors: { type: 'string' }, severity_reassessment: { type: 'string' }, confidence: { type: 'string' }, evidence: { type: 'string' } } }

var S7 = { type: 'object', properties: { was_ever_in_package_json: { type: 'boolean' }, removal_commit: { type: 'string' }, removal_reason: { type: 'string' }, readme_updated_same_time: { type: 'boolean' }, main_process_uses_electron_store: { type: 'boolean' }, in_node_modules_as_transitive: { type: 'boolean' }, conclusion: { type: 'string' }, confidence: { type: 'string' }, evidence: { type: 'string' } } }

var S_REVIEW = { type: 'object', properties: { items: { type: 'array', items: { type: 'object' } } } }

var S_SKEPTIC = { type: 'object', properties: { items: { type: 'array', items: { type: 'object' } } } }

var S_FINAL = { type: 'object', properties: { final_answers: { type: 'array', items: { type: 'object' } } } }

// ─── Phase 1: Deep Trace ───

phase('Deep Trace')

var P1 = 'Determine definitively whether the 4 kg_nodes.updated_at SQL queries at F:\\\\openloom\\\\backend\\\\crates\\\\loom-cli\\\\src\\\\memory.rs lines 1787, 1876, 1894, 1985 are reachable in the production serve code path.\n\n' +
'STEPS:\n' +
'1. Read each of the 4 functions containing these queries. Identify the function names.\n' +
'2. Search the entire codebase for callers of these functions. Do they chain back to the serve path or chat path?\n' +
'3. Check if these functions are called via orchestrator.rs or directly from dispatch handlers.\n' +
'4. Determine: are they only called from CLI diagnostic subcommands, or are they triggered by JSON-RPC dispatch handlers the frontend uses?\n' +
'5. If via JSON-RPC, check if the frontend actually invokes those RPC methods.\n' +
'6. Read the actual column name in the schema (migrations/memory/V1__memory.sql) and any ALTER TABLE that might have renamed it.\n\n' +
'Return JSON with fields: function_names, call_chain, reachable_in_serve, reachable_via_jsonrpc, frontend_invokes, actual_column_name, crash_confirmed, confidence, evidence.'

var P2 = 'Assess whether the missing composite index on message_history(session_id, seq) causes observable performance degradation in typical openLoom usage.\n\n' +
'STEPS:\n' +
'1. Read F:\\\\openloom\\\\migrations\\\\session\\\\V1__session.sql for the message_history schema.\n' +
'2. Search all SQL queries on message_history in ALL Rust source files. Identify every query pattern.\n' +
'3. Estimate table size: typical user with 100 sessions x 500 messages = 50,000 rows. Without index, every history load full scans.\n' +
'4. Check if there is any code-level caching that mitigates repeated DB reads.\n' +
'5. Check if session_id alone has an index that might reduce scan scope.\n' +
'6. Determine: for typical chat UX (loading last 50 messages from a 500-message session), would a user notice?\n\n' +
'Return JSON: query_patterns, existing_indexes, estimated_max_rows, caching_mitigation, sqlite_would_use_partial_scan, user_noticeable, recommended, confidence, evidence.'

var P3 = 'Determine whether @tiptap packages in F:\\\\openloom\\\\frontend\\\\package.json are truly unused or loaded via dynamic/lazy patterns.\n\n' +
'STEPS:\n' +
'1. Read frontend/package.json and list all @tiptap dependencies with versions.\n' +
'2. Search ALL .ts, .tsx, .js, .jsx, .json files in frontend/ for "tiptap" (case-insensitive).\n' +
'3. Check for: direct imports, dynamic imports (import()), React.lazy, require(), string references in config.\n' +
'4. Check Vite/electron-vite config for externalization mentioning tiptap.\n' +
'5. Check if @tiptap packages exist on disk in node_modules.\n' +
'6. Search git log for when tiptap was added: git log --all -p -- frontend/package.json, grep for tiptap.\n' +
'7. Search .tsx files for rich text editor patterns that might USE tiptap without importing it.\n\n' +
'Return JSON: tiptap_packages, installed_on_disk, import_count, dynamic_import_found, vite_config_reference, git_added_date, planned_usage_evidence, conclusion, confidence, evidence.'

var P4 = 'Determine whether the V8 migration was deliberately excluded from production or is a missed integration.\n\n' +
'STEPS:\n' +
'1. Read F:\\\\openloom\\\\migrations\\\\V8__add_knowledge_graph.sql completely.\n' +
'2. Read F:\\\\openloom\\\\backend\\\\crates\\\\loom-memory\\\\src\\\\memory_db.rs open() function. Note what migrations ARE applied.\n' +
'3. Read test setup in graph.rs around line 2012 — how does it use V8?\n' +
'4. Search git log for commits mentioning V8 or knowledge_graph migration.\n' +
'5. Compare V1 vs V8 schema: is V8 a SUPERSET (adds columns/indexes) or RESTRUCTURE (incompatible changes)?\n' +
'6. Check if production code references V8-only columns (source, metadata on kg_nodes).\n' +
'7. Check evidence table: V1 uses compound key, V8 uses AUTOINCREMENT id. Breaking change?\n\n' +
'Return JSON: v8_changes_summary, v1_comparison, production_code_references_v8_columns, test_only_assertions, git_commit_history, developer_intent, safe_to_apply, recommended_action, confidence, evidence.'

var P5 = 'Determine whether the 9 TODO Planned CanonicalSkillMetadata fields have active development plans or are speculative.\n\n' +
'STEPS:\n' +
'1. Read F:\\\\openloom\\\\backend\\\\crates\\\\loom-skills\\\\src\\\\lib.rs completely.\n' +
'2. Search for ANY runtime usage of these 9 fields across all .rs files.\n' +
'3. Check git log for commits touching loom-skills/.\n' +
'4. Check if these fields come from Claude Code or OpenClaw SKILL.md spec.\n' +
'5. Check if fields are PARSED from SKILL.md but not wired to runtime, or purely TODO stubs.\n' +
'6. Check for roadmap file or issue tracker references.\n\n' +
'Return JSON: field_status, total_parsed_not_consumed, total_not_even_parsed, git_activity_level, roadmap_reference, recommendation, confidence, evidence.'

var P6 = 'Determine whether the prune_stale() cascade ordering bug is actually reachable in typical operation.\n\n' +
'STEPS:\n' +
'1. Read F:\\\\openloom\\\\backend\\\\crates\\\\loom-memory\\\\src\\\\graph.rs prune_stale() around line 906-932.\n' +
'2. Read consolidation.rs run_cycle() step 4 around line 181-201.\n' +
'3. What defines a stale node? Do stale nodes typically have edges/aliases?\n' +
'4. Check if PRAGMA foreign_keys = ON is set in memory_db.rs. If OFF, ordering is irrelevant.\n' +
'5. Check actual FK definitions in V1 schema. Are there FK constraints on kg_edges referencing kg_nodes?\n' +
'6. Search for any error logs or bug reports about prune/consolidation failures.\n\n' +
'Return JSON: prune_stale_order, consolidation_order, stale_node_definition, stale_nodes_typically_have_edges, foreign_keys_enabled, fk_constraints_exist_in_v1, crash_reachable, silent_failure_reachable, observed_errors, severity_reassessment, confidence, evidence.'

var P7 = 'Determine whether electron-store was previously a dependency that was removed but README was never updated.\n\n' +
'STEPS:\n' +
'1. Search git log for electron-store: git log --all --oneline -S "electron-store" -- frontend/package.json\n' +
'2. Check git log for frontend/package.json history.\n' +
'3. If electron-store WAS in package.json, find removal commit. Read commit message and diff.\n' +
'4. Check if README was updated in same commit that removed electron-store.\n' +
'5. Read F:\\\\openloom\\\\frontend\\\\src\\\\main\\\\store.ts — does main process use electron-store?\n' +
'6. Check node_modules for electron-store as transitive dependency.\n\n' +
'Return JSON: was_ever_in_package_json, removal_commit, removal_reason, readme_updated_same_time, main_process_uses_electron_store, in_node_modules_as_transitive, conclusion, confidence, evidence.'

const deepTrace = await parallel([
  function() { return agent(P1, { label: 'trace-updated-at', schema: S1 }) },
  function() { return agent(P2, { label: 'trace-msg-hist-perf', schema: S2 }) },
  function() { return agent(P3, { label: 'trace-tiptap', schema: S3 }) },
  function() { return agent(P4, { label: 'trace-v8-migration', schema: S4 }) },
  function() { return agent(P5, { label: 'trace-skill-fields', schema: S5 }) },
  function() { return agent(P6, { label: 'trace-prune-order', schema: S6 }) },
  function() { return agent(P7, { label: 'trace-electron-store', schema: S7 }) }
])

// ─── Phase 2: Neutral Review ───

phase('Neutral Review')

var reviewPrompt = 'You are a NEUTRAL CODE REVIEWER with no stake in the outcome. Review ALL 7 deep-trace findings below and assess each independently.\n\n' +
'Findings from deep trace:\n' + JSON.stringify(deepTrace || {}) + '\n\n' +
'For each of the 7 items:\n' +
'1. Check if the evidence supports the conclusion\n' +
'2. Identify any gaps in the investigation\n' +
'3. Rate evidence quality: strong / adequate / weak\n' +
'4. Give your own independent conclusion\n\n' +
'Return JSON: { items: [{id, original_conclusion, evidence_quality, gaps, independent_conclusion, agree_with_original}] }'

const neutralReview = await agent(reviewPrompt, { label: 'neutral-review', schema: S_REVIEW })

// ─── Phase 3: Adversarial Panel ───

phase('Adversarial Panel')

var advA = 'You are SKEPTIC A. Attack conclusions about items 1, 2, and 3.\n\n' +
'Item 1 (updated_at crash):\n' + JSON.stringify((deepTrace || [])[0]) + '\n\n' +
'Item 2 (message_history perf):\n' + JSON.stringify((deepTrace || [])[1]) + '\n\n' +
'Item 3 (tiptap usage):\n' + JSON.stringify((deepTrace || [])[2]) + '\n\n' +
'For each: try to REFUTE. Find logical flaws, missing evidence, alternative explanations. Be ruthless.\n' +
'Return JSON: { items: [{item_id, attempted_refutation, refutation_strength, original_conclusion_stands, revised_severity}] }'

var advB = 'You are SKEPTIC B. Attack conclusions about items 4 and 5.\n\n' +
'Item 4 (V8 migration):\n' + JSON.stringify((deepTrace || [])[3]) + '\n\n' +
'Item 5 (skill TODO fields):\n' + JSON.stringify((deepTrace || [])[4]) + '\n\n' +
'For each: try to REFUTE. Find logical flaws, missing evidence, alternative explanations. Be ruthless.\n' +
'Return JSON: { items: [{item_id, attempted_refutation, refutation_strength, original_conclusion_stands, revised_severity}] }'

var advC = 'You are SKEPTIC C. Attack conclusions about items 6 and 7.\n\n' +
'Item 6 (prune_stale ordering):\n' + JSON.stringify((deepTrace || [])[5]) + '\n\n' +
'Item 7 (electron-store history):\n' + JSON.stringify((deepTrace || [])[6]) + '\n\n' +
'Neutral reviewer conclusions:\n' + JSON.stringify(neutralReview || {}) + '\n\n' +
'For each: try to REFUTE. Find logical flaws, missing evidence, alternative explanations. Be ruthless.\n' +
'Return JSON: { items: [{item_id, attempted_refutation, refutation_strength, original_conclusion_stands, revised_severity}] }'

const adversarialPanel = await parallel([
  function() { return agent(advA, { label: 'skeptic-a', schema: S_SKEPTIC }) },
  function() { return agent(advB, { label: 'skeptic-b', schema: S_SKEPTIC }) },
  function() { return agent(advC, { label: 'skeptic-c', schema: S_SKEPTIC }) }
])

// ─── Phase 4: Final Synthesis ───

phase('Final Synthesis')

var synPrompt = 'You are the final arbiter. Synthesize ALL evidence into definitive conclusions for the 7 uncertain items.\n\n' +
'DEEP TRACE:\n' + JSON.stringify(deepTrace || {}) + '\n\n' +
'NEUTRAL REVIEW:\n' + JSON.stringify(neutralReview || {}) + '\n\n' +
'SKEPTIC PANEL:\n' + JSON.stringify(adversarialPanel || {}) + '\n\n' +
'For EACH item, produce a FINAL answer:\n' +
'1. Clear one-sentence conclusion\n' +
'2. Confidence: high / medium / low\n' +
'3. Remaining uncertainty\n' +
'4. Recommended action\n' +
'5. Severity change: upgraded / downgraded / unchanged, with new severity\n\n' +
'Return JSON: { final_answers: [{item_id, question, conclusion, confidence, remaining_uncertainty, recommended_action, severity_change, new_severity}] }'

const finalSynthesis = await agent(synPrompt, { label: 'final-synthesis', schema: S_FINAL })

return {
  deepTrace: deepTrace,
  neutralReview: neutralReview,
  adversarialPanel: adversarialPanel,
  finalSynthesis: finalSynthesis
}
