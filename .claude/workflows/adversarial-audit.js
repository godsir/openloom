export const meta = {
  name: 'adversarial-audit',
  description: 'Comprehensive adversarial audit: README accuracy, API dispatch, architecture, frontend, migrations',
  phases: [
    { title: 'Fact Check README', detail: 'Verify every README claim against actual code' },
    { title: 'Dispatch vs API Doc', detail: 'Cross-check dispatch handlers vs docs/api.md' },
    { title: 'Architecture Integrity', detail: 'Verify crate dependency graph, naming consistency, dead code' },
    { title: 'Frontend Coherence', detail: 'Check component inventory, i18n coverage, services' },
    { title: 'Migrations & Schema', detail: 'Verify migration files, FK constraints, DB schema alignment' },
    { title: 'Adversarial Verify', detail: 'Skeptical review — try to refute every Phase 1 finding' },
    { title: 'Synthesize', detail: 'Merge confirmed findings into ranked recommendations' }
  ]
}

const README_SCHEMA = {
  "type": "object",
  "properties": {
    "verified": { "type": "array", "items": { "type": "string" } },
    "mismatches": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "claim": { "type": "string" },
          "actual": { "type": "string" },
          "severity": { "type": "string" }
        }
      }
    },
    "missing_evidence": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "claim": { "type": "string" },
          "note": { "type": "string" }
        }
      }
    },
    "stale_content": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "what": { "type": "string" },
          "fix": { "type": "string" }
        }
      }
    }
  }
}

const DISPATCH_SCHEMA = {
  "type": "object",
  "properties": {
    "dispatch_files": { "type": "array", "items": { "type": "object" } },
    "doc_only_methods": { "type": "array", "items": { "type": "string" } },
    "code_only_methods": { "type": "array", "items": { "type": "string" } },
    "param_mismatches": { "type": "array", "items": { "type": "object" } },
    "missing_sections": { "type": "array", "items": { "type": "string" } },
    "extra_sections": { "type": "array", "items": { "type": "string" } }
  }
}

const ARCH_SCHEMA = {
  "type": "object",
  "properties": {
    "dependency_issues": { "type": "array", "items": { "type": "object" } },
    "naming_inconsistencies": { "type": "array", "items": { "type": "object" } },
    "dead_code_candidates": { "type": "array", "items": { "type": "object" } },
    "tech_debt_summary": { "type": "object" },
    "undocumented_crates": { "type": "array", "items": { "type": "string" } },
    "circular_deps": { "type": "array", "items": { "type": "string" } }
  }
}

const FRONTEND_SCHEMA = {
  "type": "object",
  "properties": {
    "component_summary": { "type": "object" },
    "i18n_coverage": { "type": "object" },
    "service_coverage": { "type": "object" },
    "ipc_bridge": { "type": "object" },
    "store_inventory": { "type": "object" },
    "electron_main": { "type": "object" }
  }
}

const MIGRATION_SCHEMA = {
  "type": "object",
  "properties": {
    "migration_files": { "type": "array", "items": { "type": "string" } },
    "naming_issues": { "type": "array", "items": { "type": "object" } },
    "schema_issues": { "type": "array", "items": { "type": "object" } },
    "missing_indexes": { "type": "array", "items": { "type": "object" } },
    "fk_integrity": { "type": "array", "items": { "type": "object" } },
    "db_split_alignment": { "type": "string" }
  }
}

const VERIFY_SCHEMA = {
  "type": "object",
  "properties": {
    "confirmed_issues": { "type": "array", "items": { "type": "object" } },
    "refuted_issues": { "type": "array", "items": { "type": "object" } },
    "nuanced_issues": { "type": "array", "items": { "type": "object" } },
    "false_negatives": { "type": "array", "items": { "type": "object" } }
  }
}

const SYNTHESIS_SCHEMA = {
  "type": "object",
  "properties": {
    "overall_health": { "type": "number" },
    "health_summary": { "type": "string" },
    "high_severity": { "type": "array", "items": { "type": "object" } },
    "medium_severity": { "type": "array", "items": { "type": "object" } },
    "low_severity": { "type": "array", "items": { "type": "object" } },
    "positive_findings": { "type": "array", "items": { "type": "string" } },
    "uncertain_items": { "type": "array", "items": { "type": "string" } },
    "counts": { "type": "object" }
  }
}

phase('Fact Check README')

const readmeFactCheck = await agent(
  'You are an adversarial fact-checker. Read F:\\\\openloom\\\\README.md completely, then verify EVERY claim against the actual codebase.\n\n' +
  '1. Crate count & names: README says "15 crates". List ALL crates in backend/crates/ and compare.\n' +
  '2. Crate descriptions: For each README crate description, verify against its Cargo.toml description field.\n' +
  '3. Frontend component directories: README lists components/app, chat, input, write, kg, plan, todo, settings, shared + i18n + services. Verify each exists.\n' +
  '4. CLI params: Verify every CLI parameter for "loom chat" and "loom serve" exists in backend/crates/loom-cli/src/.\n' +
  '5. JSON-RPC method count: README says "136 methods". Count methods in docs/api.md.\n' +
  '6. API category method counts: Verify each category count in README table matches docs/api.md.\n' +
  '7. Push events: README lists 9 push events. Search for push event emissions in loom-server/src/.\n' +
  '8. Data directory: Verify ~/.loom/ paths match what code actually creates.\n' +
  '9. Tech stack versions: Verify React 19, Tailwind 4, Vite 6, Zustand 5, Electron 38 against actual package.json.\n' +
  '10. Feature claims: For each feature section, find at least one code reference supporting the claim.\n' +
  'Return JSON with fields: verified, mismatches, missing_evidence, stale_content.',
  { label: 'factcheck-readme', schema: README_SCHEMA }
)

phase('Dispatch vs API Doc')

const dispatchCheck = await agent(
  'Cross-check every dispatch handler file in F:\\\\openloom\\\\backend\\\\crates\\\\loom-server\\\\src\\\\dispatch\\\\ against docs/api.md.\n\n' +
  'For EACH dispatch file: count methods, verify every method appears in docs/api.md, verify every doc method has a handler. Check param type consistency.\n' +
  'Expected dispatch files: chat.rs, clawhub.rs, completion.rs, cron.rs, kg.rs, lsp.rs, mcp.rs, mod.rs, model.rs, plan.rs, plugins.rs, session.rs, skills.rs, system.rs, tool.rs, vfs.rs\n' +
  'Return JSON with fields: dispatch_files, doc_only_methods, code_only_methods, param_mismatches, missing_sections, extra_sections.',
  { label: 'dispatch-vs-api-doc', schema: DISPATCH_SCHEMA }
)

phase('Architecture Integrity')

const archCheck = await agent(
  'Audit the openLoom Rust codebase at F:\\\\openloom\\\\backend\\\\crates\\\\ for integrity.\n\n' +
  '1. Crate dependency graph: For each crate read Cargo.toml. Check for circular deps, missing logical deps.\n' +
  '2. Naming consistency: Check crate name matches dir name and import pattern (loom_xxx).\n' +
  '3. Dead code: Search for pub functions never called, structs never instantiated, enum variants never matched.\n' +
  '4. TODO/FIXME/HACK count across all crates.\n' +
  '5. Crate-level docs: Which crates have missing module-level docs.\n' +
  'Return JSON with fields: dependency_issues, naming_inconsistencies, dead_code_candidates, tech_debt_summary, undocumented_crates, circular_deps.',
  { label: 'audit-architecture', schema: ARCH_SCHEMA }
)

phase('Frontend Coherence')

const frontendCheck = await agent(
  'Audit the openLoom frontend at F:\\\\openloom\\\\frontend\\\\ for completeness.\n\n' +
  '1. Component inventory: List every .tsx file in src/renderer/src/components/. Compare with README.\n' +
  '2. i18n: Check zh-CN.ts, zh-TW.ts, en-US.ts all exist. Count keys. Find missing keys. Search for hardcoded Chinese strings in .tsx.\n' +
  '3. Services: List all files in src/renderer/src/services/. Verify each is imported somewhere.\n' +
  '4. IPC bridge: Count contextBridge APIs in src/preload/. README says "27".\n' +
  '5. Stores: List all Zustand stores in src/renderer/src/stores/. Check for orphans.\n' +
  '6. Electron main: List files in src/main/. Verify window, updater, tray, pet.\n' +
  'Return JSON with fields: component_summary, i18n_coverage, service_coverage, ipc_bridge, store_inventory, electron_main.',
  { label: 'audit-frontend', schema: FRONTEND_SCHEMA }
)

phase('Migrations & Schema')

const migrationCheck = await agent(
  'Audit database migrations at F:\\\\openloom\\\\migrations\\\\ and backend schema code.\n\n' +
  '1. List all migration files. Check naming (V<number>__<description>.sql). Gaps?\n' +
  '2. For each migration: check IF NOT EXISTS, FK ON DELETE, backwards compatibility.\n' +
  '3. Three-DB split: Do SQL statements target correct logical DB (loom/memory/session)?\n' +
  '4. Index coverage: Check FTS5. Find query patterns in loom-memory/src/ and loom-core/src/ missing indices.\n' +
  '5. Transaction safety: Are migrations wrapped in transactions?\n' +
  'Return JSON with fields: migration_files, naming_issues, schema_issues, missing_indexes, fk_integrity, db_split_alignment.',
  { label: 'audit-migrations', schema: MIGRATION_SCHEMA }
)

phase('Adversarial Verify')

const adversarial = await parallel([
  function() {
    return agent(
      'You are a SKEPTIC. REFUTE findings from the README fact-check:\n' + JSON.stringify(readmeFactCheck || {}) + '\n\n' +
      'For each issue: re-verify independently. If CORRECT: refuted=false. If WRONG: refuted=true with why.\n' +
      'Also find FALSE NEGATIVES: unverified README claims, features not in README, outdated version numbers.\n' +
      'Return JSON: confirmed_issues, refuted_issues, nuanced_issues, false_negatives.',
      { label: 'verify-readme', schema: VERIFY_SCHEMA }
    )
  },
  function() {
    return agent(
      'You are a SKEPTIC. REFUTE findings from architecture and dispatch audits.\n\nArchitecture:\n' + JSON.stringify(archCheck || {}) + '\n\nDispatch:\n' + JSON.stringify(dispatchCheck || {}) + '\n\n' +
      'For each issue: re-verify independently. If CORRECT: refuted=false. If WRONG: refuted=true.\n' +
      'Check dead code via grep. Check unused Cargo.toml deps.\n' +
      'Return JSON: confirmed_issues, refuted_issues, nuanced_issues, false_negatives.',
      { label: 'verify-arch-dispatch', schema: VERIFY_SCHEMA }
    )
  },
  function() {
    return agent(
      'You are a SKEPTIC. REFUTE findings from frontend and migration audits.\n\nFrontend:\n' + JSON.stringify(frontendCheck || {}) + '\n\nMigrations:\n' + JSON.stringify(migrationCheck || {}) + '\n\n' +
      'For each issue: re-verify independently. If CORRECT: refuted=false. If WRONG: refuted=true.\n' +
      'Also check: unused package.json deps, unused TS types, .gitignore coverage.\n' +
      'Return JSON: confirmed_issues, refuted_issues, nuanced_issues, false_negatives.',
      { label: 'verify-fe-mig', schema: VERIFY_SCHEMA }
    )
  }
])

phase('Synthesize')

const synthesis = await agent(
  'Synthesize ALL findings into a single ranked report.\n\n' +
  'README:\n' + JSON.stringify(readmeFactCheck || {}) + '\n\n' +
  'Dispatch:\n' + JSON.stringify(dispatchCheck || {}) + '\n\n' +
  'Architecture:\n' + JSON.stringify(archCheck || {}) + '\n\n' +
  'Frontend:\n' + JSON.stringify(frontendCheck || {}) + '\n\n' +
  'Migrations:\n' + JSON.stringify(migrationCheck || {}) + '\n\n' +
  'Verification:\n' + JSON.stringify(adversarial || {}) + '\n\n' +
  'INSTRUCTIONS: Merge and deduplicate all confirmed issues. Rank by severity. Include category/description/files/fix for each. Count totals. Note healthy aspects and uncertainties. Score overall health 1-10.\n' +
  'Return JSON: overall_health, health_summary, high_severity, medium_severity, low_severity, positive_findings, uncertain_items, counts.',
  { label: 'synthesize-report', schema: SYNTHESIS_SCHEMA }
)

return {
  readme: readmeFactCheck,
  dispatch: dispatchCheck,
  architecture: archCheck,
  frontend: frontendCheck,
  migrations: migrationCheck,
  verification: adversarial,
  synthesis: synthesis
}
