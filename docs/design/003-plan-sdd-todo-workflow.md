# 003 — Plan / SDD / Todo Workflow

## Status: Design (Phase 2)

---

## Table of Contents

1. [Overview & User Journey](#1-overview--user-journey)
2. [Architecture Diagram](#2-architecture-diagram)
3. [Backend Design](#3-backend-design)
4. [Frontend Design](#4-frontend-design)
5. [Data Flow — End-to-End Trace](#5-data-flow--end-to-end-trace)
6. [File Manifest](#6-file-manifest)
7. [Implementation Phases](#7-implementation-phases)
8. [Testing Strategy](#8-testing-strategy)
9. [Open Questions & Notes](#9-open-questions--notes)

---

## 1. Overview & User Journey

### What We Are Building

A structured **Requirements --> Plan --> Execution** pipeline modeled after DeepSeek-GUI's plan/todo system, adapted to openLoom's architecture.

Current state: "plan" mode is purely a permission constraint (read_only + LLM instruction). There is no persistent plan artifact, no todo tracking, and no linkage between plan items and execution progress.

Target state: User types `/plan "Add dark mode toggle"` -> LLM analyzes codebase, generates a structured `.loom/plans/<id>.md` file with checkbox items -> checkboxes auto-sync to a thread-local TodoPanel -> user watches progress as agent executes each checkbox.

### User Journey (3-Phase Pipeline)

```
Phase 1: DRAFT          Phase 2: CLARIFY       Phase 3: EXECUTE
┌──────────────┐       ┌──────────────┐       ┌──────────────┐
│ /plan "Add.." │  ──>  │ AI asks      │  ──>  │ Agent reads  │
│              │       │ clarifying   │       │ plan.md,     │
│ LLM analyzes │       │ questions    │       │ executes      │
│ codebase,    │       │ about scope, │       │ checkboxes   │
│ writes draft │       │ edge cases   │       │ one by one   │
│ plan.md      │       │              │       │              │
└──────┬───────┘       └──────┬───────┘       └──────┬───────┘
       │                      │                      │
       v                      v                      v
  PlanPanel              ChatArea                TodoPanel
  (right sidebar,       (normal chat,           (right sidebar,
   shows plan.md)        plan-injected          live checkbox
                         as context)             sync)
```

**Phase 1 -- Draft**: User invokes `/plan <request>`. The backend injects a plan-drafting system prompt instructing the LLM to analyze the codebase and call a `plan.create` tool with the draft content. The plan is written as a markdown file with `- [ ]` checkboxes under `.loom/plans/<uuid>.md`.

**Phase 2 -- Clarify**: User reviews the draft in PlanPanel. The plan context is injected into subsequent chat turns. The LLM can suggest refinements. User can edit the plan text directly or ask the LLM to modify it.

**Phase 3 -- Execute**: User invokes `/execute` (or a "Start Plan" button in PlanPanel). The agent reads the plan file, works through each `- [ ]` item, toggling them to `- [x]` as it completes them. The TodoPanel reflects changes in realtime.

### Additional Commands

| Command | Behavior |
|---------|----------|
| `/plan` | Open PlanPanel (or create new plan if none exists) |
| `/plan <request>` | Create plan from request |
| `/goal <description>` | Set a thread-level goal for the current session |
| `/review` | Request code review of changes since last commit or plan baseline |
| `/execute` | Start executing the current plan |

---

## 2. Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        FRONTEND (Electron)                       │
│                                                                   │
│  ┌──────────┐  ┌───────────┐  ┌──────────┐  ┌───────────────┐  │
│  │ Sidebar  │  │ ChatArea  │  │PlanPanel │  │  TodoPanel    │  │
│  │(sessions)│  │           │  │(R side-  │  │  (R sidebar   │  │
│  │          │  │- PlanBlock│  │ bar,     │  │   tab, live   │  │
│  │          │  │  renderer │  │ markdown │  │   checklist)  │  │
│  │          │  │- GoalBlock│  │ editor)  │  │               │  │
│  └──────────┘  └───────────┘  └──────────┘  └───────────────┘  │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Zustand Stores                          │   │
│  │  plan.ts │ todo.ts │ ui.ts (extended) │ input.ts (ext.)   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                    │
│                     JSON-RPC over WebSocket                       │
└──────────────────────────────┼───────────────────────────────────┘
                               │
┌──────────────────────────────┼───────────────────────────────────┐
│                      BACKEND (Rust)                               │
│                              │                                    │
│  ┌──────────────────────────┼──────────────────────────────┐    │
│  │               loom-server (Axum)                          │    │
│  │  dispatch/mod.rs ───> dispatch/plan.rs (NEW)             │    │
│  │                       dispatch/goal.rs (NEW)             │    │
│  └──────────────────────────┼──────────────────────────────┘    │
│                              │                                    │
│  ┌──────────────────────────┼──────────────────────────────┐    │
│  │               loom-core                                    │    │
│  │  orchestrator.rs  ───> plan_prompts.rs (NEW)             │    │
│  │  slash_router.rs   ───> builtin commands support (MOD)   │    │
│  │  builtin_tools.rs  ───> create_plan tool (MOD)           │    │
│  └──────────────────────────┼──────────────────────────────┘    │
│                              │                                    │
│  ┌──────────────────────────┼──────────────────────────────┐    │
│  │               loom-types                                   │    │
│  │  plan.rs (NEW) — PlanArtifact, TodoItem, ThreadGoal      │    │
│  │  lib.rs (MOD)   — re-export plan module                  │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    Storage                                 │    │
│  │  .loom/plans/<plan_id>.md  (filesystem, markdown)         │    │
│  │  SessionStore extended with: plan_id, goal, todos         │    │
│  └─────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

---

## 3. Backend Design

### 3.1 New Types (loom-types/src/plan.rs)

All new types follow the anti-dumping-ground rules: consumers listed, no implementation logic, max 250 lines.

```rust
//! Plan, todo, and goal types for the plan/SDD/todo workflow.
//!
//! Consumers: loom-core (orchestrator, plan_prompts, builtin_tools),
//!            loom-server (dispatch/plan, dispatch/goal),
//!            frontend (Zustand stores via JSON-RPC)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A plan artifact stored as a markdown file under .loom/plans/.
///
/// Consumers: loom-core (plan_prompts, builtin_tools),
///            loom-server (dispatch/plan), frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanArtifact {
    /// UUID v7, also used as the filename: <id>.md
    pub id: String,
    /// Session this plan belongs to (maps to SessionInfo.id)
    pub session_id: String,
    /// Human-readable feature name (derived from the /plan request)
    pub feature_name: String,
    /// Workspace root path (session cwd)
    pub workspace_root: String,
    /// Relative path within .loom/plans/, e.g. "a1b2c3d4.md"
    pub relative_path: String,
    /// Absolute path on disk, computed server-side
    pub absolute_path: Option<String>,
    /// The original user request that spawned this plan
    pub source_request: String,
    /// Current lifecycle state
    pub status: PlanStatus,
    /// ISO 8601
    pub created_at: String,
    /// ISO 8601
    pub updated_at: String,
}

/// Lifecycle states for a PlanArtifact.
///
/// Consumers: loom-core, loom-server, frontend
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    #[default]
    Drafting,   // Plan is being authored by LLM
    Ready,      // Plan is complete, awaiting user review
    Refining,   // Plan is being updated per user feedback
    Building,   // Plan is being executed by agent
    Completed,  // All todo items done
    Archived,   // Plan is archived (not deleted)
    Error,      // Plan generation failed
}

/// A single todo item extracted from a plan markdown checkbox.
///
/// Consumers: loom-core (plan_prompts), loom-server (dispatch/plan), frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Deterministic ID: todo_plan_{fnv1a(plan_id:relative_path:ordinal:content_hash)}
    pub id: String,
    /// The text after the checkbox, trimmed
    pub content: String,
    /// Current status
    pub status: TodoStatus,
    /// Source plan linkage
    pub source: TodoSource,
    /// ISO 8601
    pub created_at: String,
    /// ISO 8601
    pub updated_at: String,
}

/// Status of a single todo item.
///
/// Consumers: loom-core, loom-server, frontend
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
    Cancelled,
}

/// Linkage from a todo item back to its source plan.
///
/// Consumers: loom-core (plan_prompts), loom-server, frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoSource {
    /// Always "plan" for now; extensible for SDD, manual, etc.
    pub kind: String,
    /// PlanArtifact.id
    pub plan_id: String,
    /// Relative path of the plan file
    pub relative_path: String,
    /// 0-based ordinal position in the plan markdown
    pub ordinal: usize,
    /// Hash of the content string via std DefaultHasher (for deterministic matching)
    pub content_hash: u64,
}

/// A thread-level goal set via /goal.
///
/// Consumers: loom-core, loom-server (dispatch/goal), frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadGoal {
    /// Session ID this goal belongs to
    pub session_id: String,
    /// Goal description text
    pub description: String,
    /// Status tracked alongside plan execution
    pub status: GoalStatus,
    /// ISO 8601
    pub created_at: String,
    /// ISO 8601
    pub updated_at: String,
}

/// Status of a thread goal.
///
/// Consumers: loom-core, loom-server, frontend
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[default]
    Active,
    Completed,
    Abandoned,
}
```

### 3.2 Registration in loom-types/src/lib.rs

```rust
pub mod plan;

// Under "Re-export all public types" section:
pub use plan::*;
```

### 3.3 New JSON-RPC Methods

All methods follow existing patterns: sub-handler returns `Option<Result<Value, JsonRpcError>>`.

#### 3.3.1 dispatch/plan.rs (NEW)

| Method | Params | Returns | Description |
|--------|--------|---------|-------------|
| `plan.create` | `session_id`, `feature_name`, `content` (markdown) | `PlanArtifact` | Creates plan file on disk, stores in session, returns artifact |
| `plan.update` | `plan_id`, `content` (markdown) | `PlanArtifact` | Updates plan file content, re-extracts todos |
| `plan.get` | `plan_id` | `PlanArtifact` + `content` (full markdown text) | Read plan file from disk |
| `plan.list` | `session_id` (optional) | `Vec<PlanArtifact>` | List plans, optionally filtered by session |
| `plan.delete` | `plan_id` | `{ ok: true }` | Archives plan (moves to .loom/plans/archived/) |
| `plan.approve_step` | `plan_id`, `todo_id` | `TodoItem` | User approves a completed todo item |
| `todo.list` | `session_id` | `Vec<TodoItem>` | All todo items for a session |
| `todo.update_status` | `todo_id`, `status` | `TodoItem` | Updates a todo's status (Pending/InProgress/Completed/Blocked/Cancelled) |
| `goal.set` | `session_id`, `description` | `ThreadGoal` | Sets or updates the thread goal |
| `goal.status` | `session_id` | `ThreadGoal` | Gets the current thread goal |

#### 3.3.2 dispatch/mod.rs Changes

Add to module declarations:
```rust
mod goal;
mod plan;
```

Add to `dispatch_method` match chain (after `cron::handle`):
```rust
if let Some(result) = plan::handle(state, method, &p).await {
    return result;
}
if let Some(result) = goal::handle(state, method, &p).await {
    return result;
}
```

### 3.4 Storage Strategy

**Plan files**: Filesystem under `.loom/plans/<plan_id>.md`. Each file is a standard markdown document. The file name is the plan UUID (no `.json` metadata file -- the markdown header is self-describing via a YAML frontmatter block).

**Example plan file** (`.loom/plans/a1b2c3d4e5f6.md`):
```markdown
---
id: a1b2c3d4-e5f6-7890-abcd-ef1234567890
session_id: 01987654-3210-fedc-ba98-76543210fedc
feature_name: Add dark mode toggle
status: ready
created_at: 2026-06-08T10:30:00Z
updated_at: 2026-06-08T10:35:00Z
---

# Plan: Add dark mode toggle

## Overview
Add a dark mode toggle to the settings panel that persists...

## Implementation Steps

- [ ] 1. Add ThemeProvider context with dark/light state
- [ ] 2. Define CSS custom properties for dark theme colors
- [ ] 3. Add toggle component to SettingsModal
- [ ] 4. Wire toggle to ThemeProvider
- [ ] 5. Persist preference to localStorage
- [ ] 6. Add system preference detection (prefers-color-scheme)
```

**Session-level metadata**: Plan IDs and thread goals stored in an extended `SessionData` struct in `SessionStore`:

```rust
// In dispatch/session.rs, extend SessionData:
pub struct SessionData {
    // ... existing fields ...
    /// Active plan ID for this session
    pub active_plan_id: Option<String>,
    /// Thread goal for this session
    pub goal: Option<ThreadGoal>,
}
```

> **Pre-implementation verification**: Before implementation, verify that `SessionData` exists in `loom-server/src/dispatch/session.rs`. If it does not exist, define it as the per-session metadata container. If it exists but lacks `active_plan_id` / `goal` fields, extend it with `Option<String>` fields. Fallback: if `SessionData` does not exist and cannot be added, store plan/goal pointers in the existing session metadata or a separate in-memory `HashMap<String, PlanGoalState>` in the orchestrator, keyed by session ID.

**Rationale for filesystem + in-memory hybrid**:
- Plan markdown files are user-editable, git-trackable, and human-readable
- Session-level pointers (active_plan_id, goal) stay in memory for fast access
- No SQLite migration needed; plans are self-contained files
- YAML frontmatter in markdown files makes them parseable as structured data when needed

### 3.5 Plan Todo Extraction Algorithm (Backend)

When a plan file is created or updated, the backend extracts todo items:

```
fn extract_todos_from_markdown(markdown: &str, plan_id: &str, relative_path: &str) -> Vec<TodoItem> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_todo_id(plan_id: &str, relative_path: &str, ordinal: usize, content: &str) -> String {
        let mut h = DefaultHasher::new();
        format!("{plan_id}:{relative_path}:{ordinal}:{content}").hash(&mut h);
        format!("todo_plan_{:016x}", h.finish())
    }

    let mut todos = Vec::new();
    let mut ordinal = 0;

    for line in markdown.lines() {
        let trimmed = line.trim();
        // Match "- [ ] text" or "- [x] text"
        if let Some(content) = match_checkbox(trimmed) {
            // Note: DefaultHasher is NOT cryptographically secure but is sufficient
            // for deterministic todo ID generation. If cross-platform determinism is
            // needed (DefaultHasher algorithm may vary across Rust versions), use the
            // `fnv` crate instead.
            let content_hash = {
                let mut h = DefaultHasher::new();
                content.hash(&mut h);
                h.finish()
            };
            let id = hash_todo_id(plan_id, relative_path, ordinal, content);
            let is_completed = trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]");

            todos.push(TodoItem {
                id,
                content: content.to_string(),
                status: if is_completed { TodoStatus::Completed } else { TodoStatus::Pending },
                source: TodoSource {
                    kind: "plan".to_string(),
                    plan_id: plan_id.to_string(),
                    relative_path: relative_path.to_string(),
                    ordinal,
                    content_hash,
                },
                created_at: Utc::now().to_rfc3339(),
                updated_at: Utc::now().to_rfc3339(),
            });
            ordinal += 1;
        }
    }

    todos
}

fn match_checkbox(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.starts_with("- [ ] ") {
        Some(line[6..].trim())
    } else if line.starts_with("- [x] ") || line.starts_with("- [X] ") {
        Some(line[6..].trim())
    } else if line.starts_with("* [ ] ") {
        Some(line[6..].trim())
    } else if line.starts_with("* [x] ") || line.starts_with("* [X] ") {
        Some(line[6..].trim())
    } else {
        None
    }
}
```

### 3.6 Slash Router Extension

The existing `SlashRouter` only matches registered skills. We need built-in slash commands that are handled at the **orchestrator level** rather than by skill-body injection.

**Approach**: Add a `BuiltinCommands` layer in the orchestrator that runs *before* the SlashRouter. Builtin commands are intercepted and either:

1. **Short-circuit** (e.g. `/goal set ...`) -- the message never reaches the LLM; the command is handled directly
2. **Rewritten** (e.g. `/plan request`) -- the message text is replaced with a prompt-injected version that instructs the LLM to use the `create_plan` tool

```rust
// In loom-core/src/orchestrator.rs (or a new builtin_commands.rs)

struct BuiltinCommand {
    /// "/plan", "/goal", "/review", "/execute"
    prefix: &'static str,
    /// Handler: receives the full user message, returns (handled, modified_message)
    handler: fn(&str, &Orchestrator) -> BuiltinResult,
}

enum BuiltinResult {
    /// Command handled — do not invoke LLM, return this response directly
    Handled(String),
    /// Rewrite the user message, then proceed to LLM
    Rewrite(String),
    /// Not matched — fall through to SlashRouter
    Passthrough,
}
```

**Implementation** (in `process_message_with_config`):

```rust
// Before SlashRouter check:
if let Some(result) = handle_builtin_command(&combined_content, &self).await {
    match result {
        BuiltinResult::Handled(response) => return Ok(ProcessResult { response, .. }),
        BuiltinResult::Rewrite(new_content) => combined_content = new_content,
        BuiltinResult::Passthrough => {} // continue to SlashRouter
    }
}
// Then: existing SlashRouter interception...
```

**Command table**:

| Command | Handler Type | Action |
|---------|-------------|--------|
| `/plan` (no args) | `Handled` | Return `{ event: "plan.open_panel" }` — frontend opens PlanPanel |
| `/plan <request>` | `Rewrite` | Replace with plan-drafting prompt (see 3.7) |
| `/goal <desc>` | `Handled` | Write goal to session, return confirmation |
| `/goal` (no args) | `Handled` | Return current goal for display |
| `/review` | `Rewrite` | Replace with code-review prompt |
| `/execute` | `Rewrite` | Replace with plan-execution prompt |

### 3.7 Plan Prompts (loom-core/src/plan_prompts.rs)

```rust
//! System prompt builders for plan drafting, refinement, and execution.
//!
//! Consumers: loom-core (orchestrator — builtin command handlers)

/// Build the system prompt for plan drafting.
/// Injects instructions for the LLM to analyze the codebase and call
/// the `create_plan` builtin tool.
pub fn build_draft_plan_prompt(
    request: &str,
    workspace_root: &str,
    plan_relative_path: &str,
) -> String {
    format!(
        r#"## Plan Drafting Mode

You are in **Plan Drafting Mode**. Your task is to create a detailed implementation plan.

### User Request
{request}

### Instructions
1. Analyze the codebase to understand the current architecture
2. Break down the implementation into concrete, ordered steps
3. Each step MUST start with `- [ ]` (markdown checkbox)
4. Steps should be specific enough that a junior developer could execute them
5. Include file paths, function signatures, and key design decisions
6. When done, call the `create_plan` tool with:
   - operation: "create"
   - feature_name: a short name for this feature
   - content: the full markdown plan

### Plan File Location
The plan will be saved to: {workspace_root}/.loom/plans/{plan_relative_path}

### Constraints
- You are in READ_ONLY mode — do NOT modify any files
- You may read files and explore the codebase freely
- Do NOT execute any shell commands that modify the system

Start by exploring the codebase, then create the plan."#,
        request = request,
        workspace_root = workspace_root,
        plan_relative_path = plan_relative_path,
    )
}

/// Build the prompt for plan refinement based on user feedback.
pub fn build_refine_plan_prompt(
    feedback: &str,
    current_plan_content: &str,
) -> String {
    format!(
        r#"## Plan Refinement Mode

### Current Plan
{current_plan_content}

### User Feedback
{feedback}

### Instructions
1. Update the plan based on the feedback above
2. Preserve completed items (marked `- [x]`)
3. Add, remove, or reorder items as needed
4. Call the `create_plan` tool with operation: "update" and the revised content"#
    )
}

/// Build the prompt for plan execution.
pub fn build_execute_plan_prompt(
    plan_relative_path: &str,
    workspace_root: &str,
) -> String {
    format!(
        r#"## Plan Execution Mode

You are executing the plan at: {workspace_root}/.loom/plans/{plan_relative_path}

### Instructions
1. Read the plan file to get the full list of tasks
2. Work through each `- [ ]` item in order
3. After completing each item, update the checkbox to `- [x]` using FileEdit
4. If you encounter a blocked item, mark it and move to the next
5. After completing all items, report a summary

### Rules
- Execute items sequentially unless dependencies allow parallelism
- After each item is done, confirm before proceeding to the next
- If an item cannot be completed, explain why and mark it appropriately"#
    )
}
```

### 3.8 Builtin Tool: create_plan

Add to `builtin_tools.rs` a `create_plan` tool that the LLM can call during plan drafting:

```rust
// Tool definition
ToolDef {
    name: "create_plan",
    description: "Create or update a plan file in .loom/plans/. The plan is a markdown file with checkbox items.",
    parameters: json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["create", "update"],
                "description": "Whether to create a new plan or update an existing one"
            },
            "plan_id": {
                "type": "string",
                "description": "Required for 'update' operation. The plan UUID."
            },
            "feature_name": {
                "type": "string",
                "description": "Short name for the feature (required for 'create')"
            },
            "content": {
                "type": "string",
                "description": "Full markdown content of the plan"
            }
        },
        "required": ["operation", "content"]
    }),
}
```

**Tool handler flow**:
1. Parse `operation`, `content`, optional `plan_id` and `feature_name`
2. For `create`: generate UUID v7, write `.loom/plans/<id>.md` with YAML frontmatter + content
3. For `update`: overwrite existing plan file
4. Extract todos from the markdown content (see 3.5)
5. Update `SessionData.active_plan_id`
6. Emit `AgentEvent::PlanUpdated { plan_id, todos }` via EventBus
7. Return success with `PlanArtifact`

### 3.9 WebSocket Event Format

The EventBus-to-WebSocket bridge (in `loom-server/src/ws.rs`) converts `AgentEvent` variants to JSON push messages and broadcasts them to all connected clients. The WS handler must include a match arm for each new variant defined below.

#### PlanCreated

```json
{
  "type": "plan_created",
  "plan_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "title": "Add dark mode toggle",
  "workspace_root": "/home/user/project"
}
```

#### PlanUpdated

```json
{
  "type": "plan_updated",
  "plan_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "operation": "draft"
}
```

`operation` is one of: `"draft"`, `"refine"`, `"build"`.

#### GoalSet

```json
{
  "type": "goal_set",
  "session_id": "01987654-3210-fedc-ba98-76543210fedc",
  "goal": "Implement user authentication system",
  "status": "active"
}
```

#### TodoStatusChanged

```json
{
  "type": "todo_changed",
  "session_id": "01987654-3210-fedc-ba98-76543210fedc",
  "todo_id": "todo_plan_a1b2c3d4_0_12345678_abcd",
  "status": "in_progress"
}
```

`status` is one of: `"pending"`, `"in_progress"`, `"completed"`.

---

## 4. Frontend Design

### 4.1 New Zustand Slices

#### 4.1.1 stores/plan.ts (NEW)

```typescript
import { StateCreator } from 'zustand'

export interface PlanState {
  /** Currently active plan for the current session */
  activePlan: PlanArtifact | null
  /** Full markdown content of the active plan */
  planContent: string
  /** For dirty-tracking autosave */
  lastSavedContent: string
  /** Debounced save status */
  saveStatus: 'saved' | 'dirty' | 'saving' | 'error'
  /** Operation status for the current plan generation */
  operationStatus: 'idle' | 'drafting' | 'ready' | 'refining' | 'building' | 'error'
  error: string | null
  /** Whether the plan panel is in preview mode (read-only) */
  previewMode: boolean
}

export interface PlanArtifact {
  id: string
  session_id: string
  feature_name: string
  workspace_root: string
  relative_path: string
  absolute_path?: string
  source_request: string
  status: 'drafting' | 'ready' | 'refining' | 'building' | 'completed' | 'archived' | 'error'
  created_at: string
  updated_at: string
}

export interface PlanSlice extends PlanState {
  setActivePlan: (plan: PlanArtifact | null) => void
  setPlanContent: (content: string) => void
  setSaveStatus: (status: PlanState['saveStatus']) => void
  setOperationStatus: (status: PlanState['operationStatus']) => void
  setError: (error: string | null) => void
  setPreviewMode: (preview: boolean) => void
  /** Fetch plan from backend and populate state */
  loadPlan: (planId: string) => Promise<void>
  /** Persist dirty content to backend (debounced) */
  savePlan: () => Promise<void>
  /** Create a new plan via /plan command */
  createPlan: (sessionId: string, request: string) => Promise<void>
  /** Delete a plan */
  deletePlan: (planId: string) => Promise<void>
}

export const createPlanSlice: StateCreator<PlanSlice> = (set, get) => ({
  activePlan: null,
  planContent: '',
  lastSavedContent: '',
  saveStatus: 'saved',
  operationStatus: 'idle',
  error: null,
  previewMode: false,

  setActivePlan: (activePlan) => set({ activePlan }),
  setPlanContent: (planContent) => {
    const isDirty = planContent !== get().lastSavedContent
    set({ planContent, saveStatus: isDirty ? 'dirty' : 'saved' })
  },
  setSaveStatus: (saveStatus) => set({ saveStatus }),
  setOperationStatus: (operationStatus) => set({ operationStatus }),
  setError: (error) => set({ error }),
  setPreviewMode: (previewMode) => set({ previewMode }),

  loadPlan: async (planId) => {
    // RPC: plan.get { plan_id: planId }
    // Set activePlan, planContent, lastSavedContent
  },

  savePlan: async () => {
    // RPC: plan.update { plan_id, content }
    // On success: set lastSavedContent = planContent, saveStatus = 'saved'
  },

  createPlan: async (sessionId, request) => {
    // RPC: chat.send with plan-drafting system prompt injected
    // Actually: this triggers a normal chat.send where the backend
    // intercepts /plan and injects the drafting prompt
  },

  deletePlan: async (planId) => {
    // RPC: plan.delete { plan_id }
  },
})
```

#### 4.1.2 stores/todo.ts (NEW)

```typescript
import { StateCreator } from 'zustand'

export interface TodoItem {
  id: string
  content: string
  status: 'pending' | 'in_progress' | 'completed' | 'blocked' | 'cancelled'
  source: {
    kind: 'plan'
    plan_id: string
    relative_path: string
    ordinal: number
    content_hash: number
  }
  created_at: string
  updated_at: string
}

export interface ThreadGoal {
  session_id: string
  description: string
  status: 'active' | 'completed' | 'abandoned'
  created_at: string
  updated_at: string
}

export interface TodoSlice {
  /** Todos keyed by session_id */
  todosBySession: Map<string, TodoItem[]>
  /** Goals keyed by session_id */
  goalsBySession: Map<string, ThreadGoal>

  /** Hydrate todos for a session from backend */
  loadTodos: (sessionId: string) => Promise<void>
  /** Toggle a todo's status */
  toggleTodo: (sessionId: string, todoId: string) => Promise<void>
  /** Replace all todos for a session (used after plan update) */
  setTodos: (sessionId: string, todos: TodoItem[]) => void
  /** Load goal for a session */
  loadGoal: (sessionId: string) => Promise<void>
  /** Set goal for a session */
  setGoal: (sessionId: string, description: string) => Promise<void>

  // Computed helpers (not stored in state)
  getProgress: (sessionId: string) => { total: number; completed: number; pending: number }
}

export const createTodoSlice: StateCreator<TodoSlice> = (set, get) => ({
  todosBySession: new Map(),
  goalsBySession: new Map(),

  loadTodos: async (sessionId) => {
    // RPC: todo.list { session_id: sessionId }
  },

  toggleTodo: async (sessionId, todoId) => {
    // Optimistic update, then RPC: todo.update_status { todo_id, status }
  },

  setTodos: (sessionId, todos) => {
    const next = new Map(get().todosBySession)
    next.set(sessionId, todos)
    set({ todosBySession: next })
  },

  loadGoal: async (sessionId) => {
    // RPC: goal.status { session_id: sessionId }
  },

  setGoal: async (sessionId, description) => {
    // RPC: goal.set { session_id: sessionId, description }
  },

  getProgress: (sessionId) => {
    const todos = get().todosBySession.get(sessionId) ?? []
    const total = todos.length
    const completed = todos.filter(t => t.status === 'completed').length
    return { total, completed, pending: total - completed }
  },
})
```

#### 4.1.3 stores/index.ts Changes

```typescript
import { createPlanSlice, PlanSlice } from './plan'
import { createTodoSlice, TodoSlice } from './todo'

export type AppStore = /* ... existing ... */ & PlanSlice & TodoSlice

export const useStore = create<AppStore>()((...a) => ({
  /* ... existing ... */
  ...createPlanSlice(...a),
  ...createTodoSlice(...a),
}))
```

#### 4.1.4 stores/ui.ts Changes

Add panel visibility controls:
```typescript
export interface UiSlice {
  // ... existing ...
  planPanelOpen: boolean
  todoPanelOpen: boolean
  setPlanPanelOpen: (open: boolean) => void
  setTodoPanelOpen: (open: boolean) => void
}
```

### 4.2 New Components

#### 4.2.1 PlanPanel.tsx (Right Sidebar Panel)

```
┌─────────────────────────┐
│ Plan: Add Dark Mode  [x]│  ← header with feature name + close
├─────────────────────────┤
│ Status: Ready  [Execute]│  ← status badge + execute button
├─────────────────────────┤
│                         │
│  # Plan: Add Dark Mode  │
│                         │  ← Markdown editor/viewer
│  ## Overview            │     (CodeMirror or textarea)
│  ...                    │
│                         │
│  - [ ] Step 1...        │
│  - [x] Step 2...        │
│  - [ ] Step 3...        │
│                         │
├─────────────────────────┤
│ [Save] [Delete]         │  ← footer actions
└─────────────────────────┘
```

**Key behaviors**:
- Opens on the RIGHT side of ChatArea (not in the left sidebar)
- When `planPanelOpen` is true, ChatArea shrinks to accommodate
- Markdown content is editable (unless agent is actively executing)
- Changes are debounced (650ms) and auto-saved via `plan.update`
- "Execute" button sends `/execute` command
- When agent is in Building status, panel is read-only

#### 4.2.2 TodoPanel.tsx (Right Sidebar Tab)

```
┌─────────────────────────┐
│ Todos          3/7 done │  ← header with progress
├─────────────────────────┤
│ Goal: Add dark mode...  │  ← thread goal display (collapsible)
├─────────────────────────┤
│ ✅ Add ThemeProvider    │  ← completed
│ 🔄 Define CSS vars      │  ← in_progress (highlighted)
│ ⬜ Add toggle component │  ← pending
│ ⬜ Wire to provider     │
│ ⬜ Persist to storage   │
│ ⬜ System preference     │
│ ⬜ Write tests           │
├─────────────────────────┤
│ Source: plan.md  [Open] │  ← link to plan file
└─────────────────────────┘
```

**Key behaviors**:
- Click a todo to toggle: pending -> in_progress -> completed -> pending
- When a todo is toggled in the panel, update both the panel state AND the plan markdown file (via RPC)
- WebSocket events from backend update todos in realtime (agent updates plan file -> backend re-extracts -> pushes to frontend)
- Progress bar or counter in header
- "Clear completed" button to archive completed items

#### 4.2.3 PlanBlock.tsx (Chat Message Block Renderer)

Renders plan-related blocks in the chat stream. When the `create_plan` tool is called, the backend emits a `plan_created` or `plan_updated` event. The frontend renders a rich card:

```
┌─────────────────────────────────┐
│ 📋 Plan Created                  │
│ Add Dark Mode Toggle             │
│ 7 steps · created just now       │
│ [Open Plan] [View Todos]         │
└─────────────────────────────────┘
```

#### 4.2.4 GoalBlock.tsx (Chat Message Block Renderer)

```
┌─────────────────────────────────┐
│ 🎯 Goal Set                      │
│ "Add dark mode toggle to the    │
│  settings panel with system      │
│  preference detection"           │
│ Status: Active                   │
└─────────────────────────────────┘
```

### 4.3 Frontend Layout Changes

Currently, the layout is:

```
┌──────────┬──────────────────────────┐
│ Sidebar  │     ChatArea              │
│(Sessions)│                           │
│          │                           │
│          │                           │
└──────────┴──────────────────────────┘
```

New layout (when right panels are open):

```
┌──────────┬──────────────────┬─────────┐
│ Sidebar  │    ChatArea      │ Plan or │
│(Sessions)│    (shrinks)     │  Todo   │
│          │                  │ Panel   │
│          │                  │         │
└──────────┴─────────┬────────┴─────────┘
                    │
              ┌─────┴─────┐
              │  InputArea │ (full width)
              └───────────┘
```

Right panel tabs: [Plan] [Todos] — mutually exclusive or stacked. Recommendation: **tabs** with "Plan" and "Todos" toggles. When both are open simultaneously, they stack vertically with a resizable splitter.

### 4.5 Cross-Feature Integration: Right-Panel Coexistence with Feature 005

PlanPanel/TodoPanel and WriteAssistantPanel (Feature 005) both occupy the right sidebar space. The following coexistence rules apply:

- **When `appMode === 'chat'`**: The right panel shows PlanPanel/TodoPanel with tabbed toggles (`[Plan]` / `[Todos]`). The user can switch between them or view both stacked vertically with a resizable splitter.
- **When `appMode === 'write'`**: The right panel switches to WriteAssistantPanel (Feature 005), replacing the Plan/Todo tabs.
- **State preservation**: PlanPanel/TodoPanel state (active plan, plan content, todo list, scroll position) is **preserved** during mode switches — the components are hidden via CSS/conditional rendering, **not destroyed/unmounted**. When the user switches back to `'chat'` mode, the right panel restores its previous Plan/Todo view exactly as it was.
- **ModeRouter**: A `ModeRouter` component in `AppShell` manages this visibility. It reads `appMode` from the UI store and renders the appropriate right-panel content.
- **Complementary change**: See Feature 005 Amendment 2 for the corresponding WriteAssistantPanel side of this integration.

### 4.6 Plan-to-Todo Sync Algorithm (Frontend) — continued

When `plan.updated` event or `todo.list` RPC response is received:

```typescript
function mergePlanTodos(
  existing: TodoItem[],
  incoming: TodoItem[]
): TodoItem[] {
  const existingMap = new Map(existing.map(t => [t.id, t]))

  const merged: TodoItem[] = []

  for (const item of incoming) {
    const existing = existingMap.get(item.id)
    if (existing) {
      // Preserve user-set status unless the plan file explicitly changed it
      // If content hash matches, keep existing status
      // If content hash differs, use incoming (content was edited)
      if (existing.source.content_hash === item.source.content_hash) {
        merged.push({ ...item, status: existing.status })
      } else {
        merged.push(item) // Content changed, use incoming status
      }
    } else {
      merged.push(item) // New item
    }
  }

  // Items that exist but are no longer in the plan are "orphaned"
  // Keep them but mark source as removed, or remove them
  // DeepSeek-GUI: removes them from active list, keeps in history
  // openLoom strategy: remove from active list (they no longer exist in plan)

  return merged
}
```

**Deterministic ID generation** (mirrors backend, using a simple hash — if cross-platform determinism is required, use the `fnv` crate instead of JS built-in hashing):
```typescript
// Note: JS doesn't have DefaultHasher natively. Use a simple string hash
// (e.g., djb2 or the fnv npm package) that matches the backend's approach.
// The backend uses std::collections::hash_map::DefaultHasher for ID generation.
function generateTodoId(
  planId: string,
  relativePath: string,
  ordinal: number,
  content: string
): string {
  const input = `${planId}:${relativePath}:${ordinal}:${content}`
  // Use a simple 64-bit hash (djb2-64 variant or fnv package)
  const hash = hashStringToHex(input)
  return `todo_plan_${hash}`
}
```

---

## 5. Data Flow -- End-to-End Trace

### 5.1 `/plan "Add dark mode toggle"`

```
User types "/plan Add dark mode toggle" in InputArea
│
├─ frontend: sendMessage() → loomRpc('chat.send', { content: "/plan Add dark mode toggle", ... })
│
├─ backend: dispatch/chat.rs → handle_chat_send()
│   └─ orchestrator.process_message_with_config("/plan Add dark mode toggle", ...)
│       │
│       ├─ [NEW] handle_builtin_command() matches "/plan <args>"
│       │   └─ Returns BuiltinResult::Rewrite(drafting_prompt)
│       │      drafting_prompt = build_draft_plan_prompt("Add dark mode toggle", ...)
│       │
│       ├─ SlashRouter.intercept() → None (builtin already handled)
│       │
│       ├─ Agent loop runs with injected drafting prompt + create_plan tool
│       │   │
│       │   ├─ LLM explores codebase (reads files, no writes in plan mode)
│       │   ├─ LLM calls create_plan tool with operation="create", content="..."
│       │   │   └─ builtin_tools.rs: handle_create_plan()
│       │   │       ├─ Write .loom/plans/<uuid>.md
│       │   │       ├─ Extract todos from markdown
│       │   │       ├─ Update SessionData.active_plan_id
│       │   │       ├─ Emit AgentEvent::PlanCreated { plan_id, todos }
│       │   │       └─ Return PlanArtifact
│       │   │
│       │   └─ LLM reports plan summary to user
│       │
│       └─ Return ProcessResult { response: "...", ... }
│
├─ backend → WebSocket push: { type: "plan.created", plan: PlanArtifact, todos: TodoItem[] }
│
└─ frontend receives WS event:
    ├─ planStore.setActivePlan(plan)
    ├─ planStore.setPlanContent(content)
    ├─ todoStore.setTodos(sessionId, todos)
    ├─ Append PlanBlock to chat
    └─ uiStore.setPlanPanelOpen(true)  [auto-open on creation]
```

### 5.2 User Edits Plan → Todos Sync

```
User edits plan content in PlanPanel
│ (650ms debounce)
├─ planStore.savePlan() → loomRpc('plan.update', { plan_id, content })
│   │
│   └─ backend: dispatch/plan.rs → handle_plan_update()
│       ├─ Overwrite .loom/plans/<id>.md
│       ├─ Re-extract todos via extract_todos_from_markdown()
│       ├─ Push WS event: { type: "plan.updated", plan, todos }
│       │
│       └─ frontend receives WS event:
│           └─ todoStore.setTodos(sessionId, todos)  [merge with preserve]
```

### 5.3 `/execute` → Agent Executes Plan

```
User clicks "Execute" in PlanPanel
│
├─ frontend: sendMessage("/execute")
│
├─ backend: handle_builtin_command() matches "/execute"
│   └─ Returns BuiltinResult::Rewrite(execution_prompt)
│
├─ Agent loop runs with execution prompt + plan content in context
│   │
│   ├─ Agent reads plan file → sees "- [ ] Step 1..."
│   ├─ Agent executes Step 1 (e.g., creates file, edits code)
│   ├─ Agent toggles checkbox in plan file: "- [x] Step 1..."
│   │   └─ File edit triggers plan re-extraction
│   │       └─ WS push: { type: "plan.updated", todos }
│   │           └─ frontend: TodoPanel reflects ✅ Step 1
│   │
│   ├─ Agent continues to Step 2, 3, ...
│   └─ Agent reports completion
│
└─ TodoPanel shows all items ✅
```

### 5.4 `/goal "Implement user authentication system"`

```
User: "/goal Implement user authentication system"
│
├─ backend: handle_builtin_command() matches "/goal <desc>"
│   └─ Writes goal to session data
│   └─ Returns BuiltinResult::Handled("Goal set: Implement user authentication system")
│
├─ WS push: { type: "goal.set", goal: ThreadGoal }
│
└─ frontend:
    ├─ todoStore.setGoal(sessionId, goal)
    └─ TodoPanel shows goal at top
```

---

## 6. File Manifest

### New Files

| File | Purpose |
|------|---------|
| `backend/crates/loom-types/src/plan.rs` | PlanArtifact, TodoItem, ThreadGoal, all status enums |
| `backend/crates/loom-server/src/dispatch/plan.rs` | plan.* JSON-RPC handlers |
| `backend/crates/loom-server/src/dispatch/goal.rs` | goal.* JSON-RPC handlers |
| `backend/crates/loom-core/src/plan_prompts.rs` | Plan drafting/refinement/execution prompt builders |
| `backend/crates/loom-core/src/builtin_commands.rs` | `/plan`, `/goal`, `/review`, `/execute` command interceptors |
| `frontend/src/renderer/src/stores/plan.ts` | PlanStore Zustand slice |
| `frontend/src/renderer/src/stores/todo.ts` | TodoStore Zustand slice |
| `frontend/src/renderer/src/components/chat/PlanBlock.tsx` | Plan card block renderer in chat |
| `frontend/src/renderer/src/components/chat/GoalBlock.tsx` | Goal card block renderer in chat |
| `frontend/src/renderer/src/components/plan/PlanPanel.tsx` | Right sidebar plan viewer/editor |
| `frontend/src/renderer/src/components/plan/TodoPanel.tsx` | Right sidebar todo checklist |
| `frontend/src/renderer/src/components/plan/PlanPanel.module.css` | Styles for PlanPanel |
| `frontend/src/renderer/src/components/plan/TodoPanel.module.css` | Styles for TodoPanel |

### Modified Files

| File | Changes |
|------|---------|
| `backend/crates/loom-types/src/lib.rs` | Add `pub mod plan;` and `pub use plan::*;` |
| `backend/crates/loom-server/src/dispatch/mod.rs` | Add `mod plan;` `mod goal;` and delegate in `dispatch_method()` |
| `backend/crates/loom-server/src/dispatch/session.rs` | Extend `SessionData` with `active_plan_id`, `goal` |
| `backend/crates/loom-core/src/orchestrator.rs` | Add `handle_builtin_command()` call in `process_message_with_config()`; add `PlanStore` field to `Orchestrator` |
| `backend/crates/loom-core/src/builtin_tools.rs` | Add `create_plan` tool definition and handler |
| `backend/crates/loom-core/src/lib.rs` | Export `plan_prompts`, `builtin_commands` |
| `frontend/src/renderer/src/stores/index.ts` | Add PlanSlice, TodoSlice to AppStore composition |
| `frontend/src/renderer/src/stores/ui.ts` | Add `planPanelOpen`, `todoPanelOpen` toggles |
| `frontend/src/renderer/src/services/sendMessage.ts` | Detect `/plan`, `/goal`, `/review`, `/execute` prefixes; auto-open panels |
| `frontend/src/renderer/src/components/app/AppShell.tsx` | Add right panel container with PlanPanel/TodoPanel |
| `frontend/src/renderer/src/components/chat/ChatArea.tsx` | Shrink width when right panel is open |
| `frontend/src/renderer/src/components/input/InputArea.tsx` | Add slash command auto-complete hints for new commands |
| `frontend/src/renderer/src/components/app/AppShell.module.css` | New right panel layout styles |

---

## 7. Implementation Phases

### Week 1: Backend

| Day | Tasks | Deliverables |
|-----|-------|-------------|
| **1** | Types: Create `plan.rs` in loom-types, register in lib.rs | PlanArtifact, TodoItem, ThreadGoal, all enums |
| **2** | Storage: Plan file read/write, todo extraction, SessionData extension | extract_todos_from_markdown(), plan file CRUD |
| **3** | Dispatch: Create `plan.rs` and `goal.rs` handlers, register in mod.rs | 10 JSON-RPC methods, WS event push |
| **4** | Slash Commands: Create `builtin_commands.rs`, wire into orchestrator | `/plan`, `/goal`, `/review`, `/execute` intercept |
| **5** | Prompts + Tool: Create `plan_prompts.rs`, add `create_plan` builtin tool | Drafting/refinement/execution prompts, tool handler |

### Week 2: Frontend

| Day | Tasks | Deliverables |
|-----|-------|-------------|
| **1** | Stores: Create `plan.ts` and `todo.ts` slices, register in index.ts | PlanSlice, TodoSlice, UI extensions |
| **2** | PlanPanel: Markdown editor/viewer with autosave, status display | PlanPanel.tsx with CSS module |
| **3** | TodoPanel: Checklist with progress, toggle, goal display | TodoPanel.tsx with CSS module |
| **4** | Layout: Right panel integration in AppShell, ChatArea shrink, tab switching | AppShell layout, responsive widths |
| **5** | Polish: PlanBlock/GoalBlock renderers, slash command hints, edge cases, integration testing | Block components, InputArea hints |

---

## 8. Testing Strategy

### Backend Tests

| Test | Type | Covers |
|------|------|--------|
| `test_extract_todos_from_markdown` | Unit | Regex parsing of `- [ ]`, `- [x]`, `* [ ]` patterns, edge cases (empty content, nested checkboxes) |
| `test_todo_id_deterministic` | Unit | Same input produces same ID consistently |
| `test_plan_create_write_read` | Integration | Create plan file, read it back, verify YAML frontmatter + content |
| `test_plan_update_re_extracts_todos` | Integration | Update plan content, verify new todos extracted, old ones orphaned |
| `test_builtin_command_plan_no_args` | Unit | `/plan` (no args) returns event to open panel |
| `test_builtin_command_plan_with_args` | Integration | `/plan Add X` injects drafting prompt, LLM calls create_plan |
| `test_builtin_command_goal` | Unit | `/goal Set X` stores goal, `/goal` returns current goal |
| `test_dispatch_plan_create` | Integration | JSON-RPC plan.create -> file on disk -> WS event emitted |
| `test_dispatch_todo_list` | Integration | JSON-RPC todo.list returns extracted todos |
| `test_dispatch_todo_update_status` | Integration | Update todo status, verify persisted |

### Frontend Tests

| Test | Type | Covers |
|------|------|--------|
| `test_plan_store_autosave` | Unit | Dirty tracking, debounced save, saveStatus transitions |
| `test_todo_merge_preserve_status` | Unit | `mergePlanTodos()` preserves user-set status when content hash matches |
| `test_todo_merge_content_changed` | Unit | Content hash mismatch -> incoming status takes precedence |
| `test_todo_merge_orphan_removal` | Unit | Items not in incoming list are removed |
| `test_plan_panel_render` | Component | PlanPanel renders markdown, shows correct status |
| `test_todo_panel_toggle` | Component | Clicking todo toggles status, calls RPC |
| `test_right_panel_layout` | Component | Opening PlanPanel shrinks ChatArea, closing restores |
| `test_plan_block_render` | Component | PlanBlock renders plan card with metadata |

### Integration Tests

| Test | Covers |
|------|--------|
| Full `/plan` flow | User types /plan -> backend creates plan -> frontend shows PlanPanel + TodoPanel |
| Full `/execute` flow | User clicks Execute -> agent processes checkboxes -> TodoPanel updates live |
| Plan edit -> todo sync | User edits plan in PlanPanel -> todos re-extract -> TodoPanel reflects changes |
| Goal set -> display | User types /goal -> goal appears in TodoPanel header |

---

## 9. Open Questions & Notes

1. **Plan file format**: YAML frontmatter in markdown is chosen over a separate `.json` metadata file to keep plans self-contained and user-editable. Is this acceptable, or should we store metadata separately?

2. **Cross-session plans**: Currently plans are tied to sessions. Should a plan be shareable across sessions? If so, plan storage moves from `session_id` foreign key to a workspace-level registry.

3. **SDD integration**: The original spec mentions a 3-phase SDD pipeline with Draft -> Clarify -> Upgrade phases. For Phase 2, we scope to the Plan workflow only. The SDD Clarify phase (AI assistant sidebar with context) can be a follow-up feature reusing the PlanPanel architecture.

4. **Plan approval workflow**: Should individual todo items require user approval before the agent marks them complete? Or does the agent autonomously mark items done? Recommendation: agent marks autonomously during execution; user can override status in TodoPanel.

5. **Concurrent plan editing**: What happens if the user edits the plan file externally (e.g., in VS Code) while the PlanPanel is open? Recommendation: file watcher on `.loom/plans/` directory that triggers re-extraction and WS push when files change on disk.

6. **Plan archival**: Plans are archived (moved to `.loom/plans/archived/`) rather than deleted. This preserves history for future reference.

7. **EventBus integration**: The existing `EventBus` in loom-core already has an `AgentEvent` enum. We'll add variants:
   - `PlanCreated { plan_id: String, session_id: String }`
   - `PlanUpdated { plan_id: String, session_id: String, todos: Vec<TodoItem> }`
   - `GoalSet { session_id: String, goal: ThreadGoal }`
   - `TodoStatusChanged { session_id: String, todo: TodoItem }`

   The WebSocket handler in `ws.rs` already broadcasts `AgentEvent` variants to connected clients. We extend the conversion to include these new variants as JSON push messages (see section 3.9 for the exact format).

8. **UUID v7 dependency**: The design specifies UUID v7 for plan IDs (generated in the `create_plan` tool handler, section 3.8). Before implementation, verify that `uuid = "1"` with the `v7` feature is present in the workspace `Cargo.toml`. If not present, add `uuid = { version = "1", features = ["v7"] }` to the workspace dependencies, and `uuid = { workspace = true, features = ["v7"] }` to `loom-core/Cargo.toml`. If the `uuid` crate is not in the workspace at all, add it as a workspace dependency first.

---

*Document version: 1.0 — Last updated: 2026-06-08*
