# Memory System Panel Redesign - Design Spec

## Overview

Redesign the "认知图谱" (Cognitive Graph) settings panel to properly reflect the backend's memory system architecture. The panel will be renamed to "记忆系统" (Memory System) and reorganized into two tabs: Knowledge Graph and Maintenance.

## Goals

1. Align frontend terminology with backend architecture ("Memory System" instead of "Cognitive Graph")
2. Expose scope-level data visibility (global vs session-scoped)
3. Provide maintenance tools for knowledge graph cleanup
4. Show cognition records with their evolution timeline
5. Fix existing UI bugs (legend mismatch, stale stats after deletion, search-graph disconnect)

## Scope

- **In scope**: Frontend panel redesign, 4 new backend RPC methods, 2 new TypeScript types
- **Out of scope**: Backend scope infrastructure changes, new KG entity types, force-graph visualization changes

## Design

### 1. Panel Structure

**File**: `KnowledgeGraphPanel.tsx` (filename unchanged, UI label changed)

**UI Label**: "记忆系统" (Memory System)

**Tabs**:
- **知识图谱** (Knowledge Graph) - Default tab
- **维护** (Maintenance)

**Component Split**:
```
KnowledgeGraphPanel.tsx          # Container, manages tab switching
├── KnowledgeGraphTab.tsx        # Entity list + force-graph + search
└── MaintenanceTab.tsx           # Cognition records + cleanup tools
```

### 2. Knowledge Graph Tab

#### Layout

```
┌──────────────────────────────────────────────┐
│ 统计: 实体 128  关系 342                      │
│ ┌──────────────────────────────────────────┐ │
│ │ Scope: [全部 ▾]  Search: [______] [搜索] │ │
│ └──────────────────────────────────────────┘ │
│ [实体列表] [图谱视图]              [清除图谱] │
│                                              │
│ ┌──────────────────────────────────────────┐ │
│ │ Entity 1 [global] [Person] [85%]         │ │
│ │   Description...                         │ │
│ │   [邻居] [遍历] [删除]                    │ │
│ │                                          │ │
│ │ Entity 2 [session:abc123] [Tech] [90%]   │ │
│ │   ...                                    │ │
│ └──────────────────────────────────────────┘ │
│                                              │
│ 图例: Person Technology Project Concept ... │
└──────────────────────────────────────────────┘
```

#### Features

1. **Statistics Bar**: Node count + edge count (refreshed after deletions)
2. **Scope Filter**: Dropdown with options:
   - 全部 (All) - default
   - 全局 (Global only)
   - 会话级 (Session-scoped only)
3. **Search Box**: Filter entities by name, supports Enter key
4. **View Toggle**: Switch between list view and force-graph view
5. **Entity List**: Each row shows:
   - Name
   - Scope badge (hidden for "global", shows session id prefix for session-scoped)
   - Entity type badge (colored)
   - Confidence percentage
   - Description (if available)
   - Actions: 邻居 (neighbors), 遍历 (walk), 删除 (delete)
6. **Force Graph**: Existing react-force-graph-2d visualization (unchanged)
7. **Legend**: Updated to 7 types matching backend LLM extraction prompt

#### Bug Fixes

1. **Legend Alignment**: Change `ENTITY_COLORS` from 9 types to 7:
   ```typescript
   const ENTITY_COLORS = {
     Person: '#22d3ee',
     Technology: '#a78bfa',
     Topic: '#fb923c',
     Project: '#34d399',
     Concept: '#f472b6',  // was Interest
     Tool: '#818cf8',
     Organization: '#fbbf24',  // new
   }
   ```
2. **Stats Refresh**: After `kgNodeDelete` or `kgEdgeDelete`, call `kgLoadStats()`
3. **Search-Graph Integration**: When search has results and user switches to graph tab, auto-walk from first search result
4. **Clear Graph Behavior**: `kgClearGraph()` only clears `kgGraph`, not `kgSearchResults`
5. **Empty Search**: Remove `if (!e.target.value) setActiveTab('list')` - don't force tab switch

### 3. Maintenance Tab

#### Layout

```
┌──────────────────────────────────────────────┐
│ 认知记录                                      │
│ ┌──────────────────────────────────────────┐ │
│ │ Subject: [USER ▾]  Scope: [全部 ▾]       │ │
│ ├──────────────────────────────────────────┤ │
│ │ ▼ trait    │ value    │ conf │ ver │ scope│ │
│ │   编程习惯  │ Rust    │ 90%  │ v3  │ global│ │
│ │ ┌────────────────────────────────────┐   │ │
│ │ │ v1: 学Rust (40%, 2024-01) →        │   │ │
│ │ │ v2: 用Rust (70%, 2024-03) →        │   │ │
│ │ │ v3: Rust (90%, 2024-05)            │   │ │
│ │ └────────────────────────────────────┘   │ │
│ │                                          │ │
│ │   喜好    │ 咖啡    │ 75%  │ v1  │ global│ │
│ └──────────────────────────────────────────┘ │
│                                              │
│ 图谱维护                                      │
│ ┌──────────────────────────────────────────┐ │
│ │ 当前统计: 实体 128, 关系 342             │ │
│ │ [清理 30 天以上低置信度实体]               │ │
│ └──────────────────────────────────────────┘ │
└──────────────────────────────────────────────┘
```

#### Features

**Cognition Records Section**:

1. **Subject Filter**: Dropdown populated by `cognitions.subjects` RPC, default "USER"
2. **Scope Filter**: Same as Knowledge Graph tab (全部/全局/会话级)
3. **Cognition List**: Table with columns:
   - Expand toggle (▶ / ▼)
   - trait name
   - current value
   - confidence (%)
   - version (v1, v2, v3...)
   - scope badge
4. **Evolution Timeline**: When a row is expanded, show all historical snapshots:
   - Ordered from oldest to newest (v1 → v2 → v3)
   - Each snapshot shows: version, value, confidence, timestamp
   - Format: `v1: 旧值 (40%, 2024-01-15) → v2: 中间值 (70%, 2024-03-20) → v3: 当前值 (90%, 2024-05-30)`
5. **Data Loading**:
   - On tab open: call `cognitionListSubjects()` to populate subject dropdown
   - On subject/scope change: call `cognitionListBySubject(subject, scope)`
   - On row expand: call `cognitionLoadSnapshots(cognition_id)`

**Graph Maintenance Section**:

1. **Stats Display**: Current node count + edge count
2. **Prune Button**: "清理 N 天以上低置信度实体"
   - Click opens confirmation dialog
   - On confirm: call `kgPrune(older_than_days)`
   - Show result: "已清理 N 个实体"
   - Refresh stats after prune

### 4. Backend RPC Additions

#### New Methods

**1. `cognitions.list`**

```rust
// Request
{
  "subject": "USER",           // required
  "scope": "global",           // optional, filter by scope
  "limit": 50,                 // optional, default 50
  "offset": 0                  // optional, default 0
}

// Response
{
  "rows": [
    {
      "id": 123,
      "subject": "USER",
      "trait_name": "programming_language",
      "value": "Rust",
      "confidence": 0.9,
      "evidence_count": 5,
      "first_seen": 1704067200,
      "last_updated": 1717027200,
      "version": 3,
      "scope": "global"
    }
  ]
}
```

Implementation: Wrap `CognitionStore::query_by_subject()`, add scope filter.

**2. `cognitions.snapshots`**

```rust
// Request
{
  "cognition_id": 123
}

// Response
{
  "snapshots": [
    {
      "id": 456,
      "version": 2,
      "trait_name": "programming_language",
      "value": "Python",
      "confidence": 0.7,
      "evidence_count": 3,
      "snapshot_at": 1711929600
    },
    {
      "id": 457,
      "version": 1,
      "trait_name": "programming_language",
      "value": "JavaScript",
      "confidence": 0.4,
      "evidence_count": 1,
      "snapshot_at": 1704067200
    }
  ]
}
```

Implementation: Wrap `CognitionStore::snapshots_for()`.

**3. `cognitions.subjects`**

```rust
// Request
{}

// Response
{
  "subjects": ["USER", "Alice", "ProjectX"]
}
```

Implementation: New query `SELECT DISTINCT subject FROM cognitions ORDER BY last_updated DESC`.

**4. `kg.prune`**

```rust
// Request
{
  "older_than_days": 30
}

// Response
{
  "pruned_count": 42
}
```

Implementation: Wrap `GraphStore::prune_stale(older_than_days, 1000)`.

#### Modified Methods

**`kg.list` and `kg.search`**: Add optional `scope` parameter to filter results.

```rust
// kg.list
{
  "limit": 50,
  "offset": 0,
  "scope": "global"  // optional filter
}
```

### 5. TypeScript Types

Add to `types/bindings.ts`:

```typescript
export interface Cognition {
  id: number
  subject: string
  trait_name: string
  value: string
  confidence: number
  evidence_count: number
  first_seen: number  // Unix timestamp
  last_updated: number
  version: number
  scope: string
}

export interface CognitionHistory {
  id: number
  version: number
  trait_name: string
  value: string
  confidence: number
  evidence_count: number
  snapshot_at: number  // Unix timestamp
}
```

### 6. Store Extensions

Add to `stores/kg.ts`:

```typescript
// State
cognitionList: Cognition[]
cognitionSubjects: string[]
cognitionSnapshots: Record<number, CognitionHistory[]>  // keyed by cognition_id

// Actions
cognitionListBySubject: (subject: string, scope?: string) => Promise<void>
cognitionListSubjects: () => Promise<void>
cognitionLoadSnapshots: (cognitionId: number) => Promise<void>
kgPrune: (olderThanDays: number) => Promise<void>
```

## Implementation Notes

1. **File Structure**: Keep `KnowledgeGraphPanel.tsx` as container, extract two new components `KnowledgeGraphTab.tsx` and `MaintenanceTab.tsx` in same directory.

2. **Scope Badge Rendering**: 
   - If `scope === 'global'`: don't show badge
   - Otherwise: show badge with truncated session id (first 6 chars)

3. **Timestamp Formatting**: Use `date-fns` or native `Date` to format Unix timestamps as `YYYY-MM-DD`.

4. **Evolution Timeline UX**: 
   - Default collapsed (no snapshots loaded)
   - On expand: load snapshots, show loading state
   - Render as horizontal timeline: `v1 → v2 → v3`
   - Each node shows: version, value (truncated), confidence, date

5. **Prune Confirmation**: Use existing `showConfirm` dialog, show warning about data loss.

6. **Backward Compatibility**: All new RPC methods are additions, no breaking changes to existing methods.

## Success Criteria

1. Panel UI label shows "记忆系统" instead of "认知图谱"
2. Knowledge Graph tab displays scope badges for session-scoped entities
3. Deletion operations refresh statistics immediately
4. Legend shows exactly 7 entity types matching backend
5. Maintenance tab shows cognition records with expandable evolution timeline
6. Prune button successfully removes stale entities and refreshes stats
7. Scope filter works in both tabs
8. Search results can transition to graph view

## Dependencies

- **Backend**: Requires 4 new RPC methods + scope parameter additions
- **Frontend**: React 19, existing `react-force-graph-2d`, `date-fns` (optional)
- **Database**: No schema changes, uses existing tables
