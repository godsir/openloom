// Hand-maintained TypeScript types mirroring the Rust loom-types structs over the JSON-RPC wire.
// Keep in sync manually with backend changes.

export type JsonRpcRequest = {
  jsonrpc: string
  method: string
  params?: unknown
  id: number
}

export type JsonRpcResponse = {
  jsonrpc: string
  result?: unknown
  error?: { code: number; message: string; data?: unknown }
  id: number
}

// === Model Config (mirrors backend ModelConfig) ===

export type ModelBackend = 'LmStudio' | 'Anthropic' | 'OpenAI' | 'DeepSeek' | 'Ollama' | 'Custom'

export interface ModelConfig {
  name: string
  model?: string
  model_type: 'Router' | 'Summarizer' | 'Reasoning'
  backend: ModelBackend
  backend_label?: string
  base_url?: string
  api_key_env?: string
  api_format?: string
  context_size: number
  max_output_tokens?: number
  is_active?: boolean
  capabilities?: ModelCapabilities
  input_price?: number
  output_price?: number
  cache_read_price?: number
  cache_write_price?: number
}

export interface ModelCapabilities {
  vision?: boolean
  reasoning?: boolean
  function_calling?: boolean
}

// === Knowledge Graph ===

export interface KgNode {
  node_id: number
  name: string
  entity_type: string
  description: string
  confidence: number
  scope: string
  layer: string
}

export interface KgEdge {
  source: string
  target: string
  relation_type: string
  fact: string
  confidence: number
}

export interface KgGraph {
  nodes: KgNode[]
  edges: KgEdge[]
}

export interface KgStats {
  node_count: number
  edge_count: number
}

export interface ModelListItem {
  name: string
  model?: string
  backend: string
  backend_label?: string
  base_url?: string
  api_key_env?: string
  api_format?: string
  is_active: boolean
  context_size?: number
  capabilities?: ModelCapabilities
  input_price?: number
  output_price?: number
  cache_read_price?: number
  cache_write_price?: number
}

export interface Cognition {
  id: number
  subject: string
  trait_name: string
  value: string
  confidence: number
  evidence_count: number
  first_seen: number
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
  snapshot_at: number
}

// === Memory Health ===

export interface MemoryHealth {
  total_nodes: number
  total_edges: number
  total_cognitions: number
  stale_nodes: number
  orphan_nodes: number
  layer_distribution: [string, number][]
  fragmentation_score: number
  status: string
  checked_at: string
}

// === Memory Quality Report ===

export interface MemoryQualityReport {
  avg_relevance: number
  injection_count: number
  turns_with_references: number
  total_entities: number
  duplicate_rate: number
  stale_entity_count: number
  avg_confidence: number
  entity_types_distribution: [string, number][]
  layer_distribution: [string, number][]
  entities_added_recently: number
  entities_accessed_recently: number
  consolidation_runs: number
  total_merged: number
  health_score: number
}

// === Persona Data ===

export interface PersonaData {
  persona: string | null
  rich_persona: RichPersona | null
}

export interface TechProficiency {
  name: string
  level: 'Beginner' | 'Intermediate' | 'Advanced' | 'Expert'
  confidence: number
  evidence_count: number
}

export interface Preference {
  key: string
  value: string
  strength: number
  evidence: string[]
}

export type GoalStatus = 'Active' | 'Achieved' | 'Abandoned'

export interface Goal {
  description: string
  status: GoalStatus
  priority: number
}

export type Approach = 'CodeFirst' | 'PlanFirst' | 'Conversational'

export type Verbosity = 'Concise' | 'Balanced' | 'Detailed'

export interface WorkingStyle {
  approach: Approach
  verbosity: Verbosity
}

export type Formality = 'Casual' | 'Neutral' | 'Formal'

export interface CommunicationStyle {
  language: string
  formality: Formality
}

export interface RichPersona {
  tech_stack: TechProficiency[]
  preferences: Preference[]
  goals: Goal[]
  working_style: WorkingStyle
  communication: CommunicationStyle
  expertise_areas: string[]
  behavioural_patterns: string[]
  last_updated: string
}

// === Session Patterns ===

export interface TopicPattern {
  topic: string
  session_count: number
  first_seen: string
  last_seen: string
}

export interface ToolPreference {
  tool: string
  usage_count: number
  avg_confidence: number
}

export interface LearningPath {
  domain: string
  stages: string[]
  confidence: number
}

export interface TimePattern {
  activity: string
  hour_bucket: number
  frequency: number
}

export interface SessionPatternReport {
  topics: TopicPattern[]
  tools: ToolPreference[]
  learning: LearningPath[]
  time_patterns: TimePattern[]
}

// === Consolidation Report ===

export interface ConsolidationReport {
  promoted: number
  demoted: number
  pruned: number
  layer_counts: [string, number][]
  summary: string
  merged_nodes: number
  merged_cognitions: number
  promoted_count: number
  edge_rerouted: number
  errors: string[]
}

// === Forgetting Report ===

export interface ForgettingReport {
  cycle_timestamp: string
  nodes_removed: number
  edges_removed: number
  cognitions_removed: number
  skipped_protected: number
  min_importance_threshold: number
  max_age_days: number
  summary: string
}

// === Pipeline Status ===

export interface PipelineStatus {
  status: string
  node_count: number
  edge_count: number
  cognition_count: number
  recent_extractions_24h: number
  last_consolidation: string
  checked_at: string
}

// === Layer Stats ===

export interface LayerStats {
  layer_name: string
  node_count: number
}
