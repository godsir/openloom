import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'
import { t as _t } from '../i18n'
import type { KgNode, KgEdge, KgGraph, KgStats, Cognition, CognitionHistory, MemoryHealth, MemoryQualityReport, PersonaData, SessionPatternReport, ConsolidationReport, ForgettingReport, PipelineStatus, LayerStats } from '../types/bindings'
import { loadAllKgNodes } from './loadAllKgNodes'

let kgListGeneration = 0
let kgGraphGeneration = 0

function hashStringForGraph(value: string): number {
  let hash = 0
  for (let i = 0; i < value.length; i++) hash = ((hash << 5) - hash + value.charCodeAt(i)) | 0
  return hash || 1
}

function shortSessionLabel(sessionId: string, title?: string | null): string {
  const cleanTitle = title?.replace(/^\[[^\]]+\]\s*/, '').trim()
  if (cleanTitle) return cleanTitle.length > 18 ? cleanTitle.slice(0, 17) + '…' : cleanTitle
  return '会话记忆'
}

export interface KgSlice {
  kgSearchResults: KgNode[]
  kgGraph: KgGraph | null
  kgSelectedNode: KgNode | null
  kgStats: KgStats | null
  kgNodeList: KgNode[]
  cognitionList: Cognition[]
  cognitionPage: number
  cognitionPageSize: number
  cognitionSubjects: string[]
  cognitionSnapshots: Record<number, CognitionHistory[]>
  memoryHealth: MemoryHealth | null
  qualityReport: MemoryQualityReport | null
  personaData: PersonaData | null
  patternReport: SessionPatternReport | null
  consolidationReport: ConsolidationReport | null
  forgettingReport: ForgettingReport | null
  pipelineStatus: PipelineStatus[]
  layerStats: LayerStats[]
  activeTab: 'graph' | 'health' | 'persona' | 'patterns' | 'maintenance'
  kgSearch: (query: string) => Promise<void>
  kgExpandNode: (nodeName: string, scope?: string) => Promise<void>
  kgWalkFrom: (startName: string, maxDepth?: number, scope?: string) => Promise<void>
  kgLoadGraph: (seeds: string[], maxDepth?: number, scope?: string, nodes?: KgNode[]) => Promise<void>
  kgLoadStats: () => Promise<void>
  kgClearGraph: () => void
  kgListNodes: (scope?: string) => Promise<KgNode[]>
  kgNodeDelete: (name: string) => Promise<void>
  kgEdgeDelete: (source: string, target: string, relation: string) => Promise<void>
  cognitionListBySubject: (subject: string, scope?: string) => Promise<void>
  cognitionSetPage: (page: number) => void
  cognitionListSubjects: () => Promise<void>
  cognitionLoadSnapshots: (cognitionId: number) => Promise<void>
  cognitionDelete: (id: number) => Promise<boolean>
  kgPrune: (olderThanDays: number) => Promise<void>
  memoryPromote: (sessionId: string, minConfidence?: number) => Promise<{ promoted_nodes: number; promoted_cognitions: number }>
  kgLoadHealth: () => Promise<void>
  kgLoadQuality: (lookbackDays?: number) => Promise<void>
  kgLoadPersona: () => Promise<void>
  kgLoadPatterns: () => Promise<void>
  kgRunConsolidation: () => Promise<void>
  kgRunForgetting: (minImportance?: number, maxAgeDays?: number) => Promise<void>
  kgLoadPipelineStatus: () => Promise<void>
  kgLoadLayerStats: () => Promise<void>
  kgPromoteToLayer: (nodeName: string, targetLayer: string) => Promise<void>
  kgSetActiveTab: (tab: 'graph' | 'health' | 'persona' | 'patterns' | 'maintenance') => void
}

export const createKgSlice: StateCreator<KgSlice> = (set, get) => ({
  kgSearchResults: [],
  kgGraph: null,
  kgSelectedNode: null,
  kgStats: null,
  kgNodeList: [],
  cognitionList: [],
  cognitionPage: 0,
  cognitionPageSize: 20,
  cognitionSubjects: [],
  cognitionSnapshots: {},
  memoryHealth: null,
  qualityReport: null,
  personaData: null,
  patternReport: null,
  consolidationReport: null,
  forgettingReport: null,
  pipelineStatus: [],
  layerStats: [],
  activeTab: 'graph',

  kgSearch: async (query) => {
    try {
      const result = await loomRpc<{ rows: KgNode[] }>('kg.search', { query, limit: 20 })
      set({ kgSearchResults: result.rows ?? [] })
    } catch (err) {
      // 失败时给出可见反馈，而非静默显示空结果误导用户（B8）
      console.error('[kgSearch] failed:', err)
      ;(get() as any).addToast?.({ type: 'error', message: _t('kg.searchFailed') })
    }
  },

  kgExpandNode: async (nodeName, scope?) => {
    const generation = ++kgGraphGeneration
    set({ kgSelectedNode: null })
    const result = await loomRpc<KgGraph>('kg.neighbors', { node_name: nodeName, limit: 30, scope })
    const graph = result as KgGraph
    if (!graph.nodes?.length && !graph.edges?.length) return

    const prev = get().kgGraph
    if (!prev) {
      const center: KgNode = (get().kgSearchResults.find(n => n.name === nodeName))
        ?? get().kgNodeList.find(n => n.name === nodeName)
        ?? { node_id: 0, name: nodeName, entity_type: 'Unknown', description: '', confidence: 1.0, scope: 'global', layer: 'episodic' }
      if (generation === kgGraphGeneration) {
        set({ kgGraph: { nodes: [center, ...graph.nodes], edges: graph.edges } })
      }
      return
    }

    const nodeMap = new Map<string, KgNode>()
    for (const n of prev.nodes) nodeMap.set(n.name, n)
    for (const n of graph.nodes) { if (!nodeMap.has(n.name)) nodeMap.set(n.name, n) }

    const edgeKey = (e: KgEdge) => `${e.source}||${e.target}||${e.relation_type}`
    const edgeSet = new Set(prev.edges.map(edgeKey))
    const newEdges = graph.edges.filter(e => !edgeSet.has(edgeKey(e)))

    if (generation === kgGraphGeneration) {
      set({
        kgGraph: {
          nodes: [...nodeMap.values()],
          edges: [...prev.edges, ...newEdges],
        },
      })
    }
  },

  kgWalkFrom: async (startName, maxDepth = 2, scope) => {
    const generation = ++kgGraphGeneration
    const result = await loomRpc<KgGraph>('kg.walk', { start_name: startName, max_depth: maxDepth, scope, limit: 50 })
    if (result.nodes?.length) {
      if (generation === kgGraphGeneration) {
        set({ kgGraph: result, kgSelectedNode: null })
      }
    } else {
      // At minimum show the start node itself so the graph isn't stuck empty
      const startNode = get().kgNodeList.find(n => n.name === startName)
      if (generation === kgGraphGeneration) {
        set({
          kgGraph: {
            nodes: startNode ? [startNode] : [{ node_id: 0, name: startName, entity_type: 'Unknown', description: '', confidence: 1.0, scope: 'global', layer: 'episodic' }],
            edges: [],
          },
          kgSelectedNode: null,
        })
      }
    }
  },

  kgLoadGraph: async (seeds, maxDepth = 2, scope, nodes) => {
    const generation = ++kgGraphGeneration
    const sourceNodes = nodes ?? get().kgNodeList
    // Session-scoped memories are separate galaxies. Qualifying render IDs by
    // scope prevents same-named entities from different sessions collapsing
    // into one node, while original_name remains available for KG actions.
    const nodesByScope = new Map<string, KgNode[]>()
    for (const node of sourceNodes) {
      const nodeScope = node.scope || 'global'
      const group = nodesByScope.get(nodeScope) || []
      group.push(node)
      nodesByScope.set(nodeScope, group)
    }
    const sessionScopes = [...nodesByScope.keys()].filter(s => s !== 'global')
    if (sessionScopes.length > 0) {
      const sessionTitles = new Map<string, string | null>(
        (((get() as any).sessions || []) as Array<{ path: string; title: string | null }>)
          .map(session => [session.path, session.title]),
      )
      const scopedGraphs = await Promise.all(
        [...nodesByScope.entries()].map(async ([nodeScope, scopeNodes]) => {
          const names = [...new Set(scopeNodes.map(n => n.name))]
          let scopeEdges: KgEdge[] = []
          try {
            const result = await loomRpc<{ edges: KgEdge[] }>('kg.edges_between', {
              node_names: names,
              scope: nodeScope,
            })
            scopeEdges = result.edges || []
          } catch (err) {
            console.error('[kgLoadGraph] scoped edge load failed:', nodeScope, err)
          }
          const prefix = nodeScope === 'global' ? '' : `${nodeScope}\u0000`
          const renderedNodes = scopeNodes.map(node => ({
            ...node,
            name: prefix + node.name,
            original_name: node.name,
          }))
          const renderedEdges = scopeEdges.map(edge => ({
            ...edge,
            source: prefix + edge.source,
            target: prefix + edge.target,
          }))

          if (nodeScope !== 'global') {
            const hubName = `${prefix}__SESSION__`
            renderedNodes.unshift({
              node_id: -Math.abs(hashStringForGraph(nodeScope)),
              name: hubName,
              original_name: shortSessionLabel(nodeScope, sessionTitles.get(nodeScope)),
              entity_type: 'Session',
              description: `Session memory galaxy: ${nodeScope}`,
              confidence: 1,
              scope: nodeScope,
              layer: 'episodic',
            })
            for (const node of scopeNodes) {
              renderedEdges.push({
                source: hubName,
                target: prefix + node.name,
                relation_type: 'session_memory',
                fact: '',
                confidence: 1,
              })
            }
          }
          return { nodes: renderedNodes, edges: renderedEdges }
        }),
      )
      if (generation === kgGraphGeneration) {
        set({
          kgGraph: {
            nodes: scopedGraphs.flatMap(graph => graph.nodes),
            edges: scopedGraphs.flatMap(graph => graph.edges),
          },
          kgSelectedNode: null,
        })
      }
      return
    }
    // ── Approach C: Galaxy-aware graph loading ───────────────────────
    // Phase 1: Walk USER depth 1 → discover galaxy centres (1-hop neighbours)
    // Phase 2: Walk each centre depth=maxDepth → build its galaxy
    // Phase 3: Filter cross-galaxy edges (keep only intra-galaxy + USER edges)
    const nodeMap = new Map<string, KgNode>()
    const edgeMap = new Map<string, KgEdge>()
    const nodeGalaxy = new Map<string, number>() // galaxyId per node (0 = USER core)

    const edgeKey = (e: KgEdge) => `${e.source}||${e.target}||${e.relation_type}`

    const addNode = (n: KgNode, galaxyId: number) => {
      if (nodeMap.has(n.name)) return
      nodeMap.set(n.name, n)
      nodeGalaxy.set(n.name, galaxyId)
    }

    const addEdge = (e: KgEdge) => {
      const key = edgeKey(e)
      if (!edgeMap.has(key)) edgeMap.set(key, e)
    }

    const addResult = (r: KgGraph, galaxyId: number) => {
      for (const n of r.nodes || []) addNode(n, galaxyId)
      for (const e of r.edges || []) addEdge(e)
    }

    // ── Phase 1: Walk USER depth 1 to discover galaxy centres ──────
    let userResult: KgGraph | null = null
    try {
      userResult = await loomRpc<KgGraph>('kg.walk', {
        start_name: 'USER', max_depth: 1, scope, limit: 100,
      })
    } catch (err) {
      console.error('[kgLoadGraph] USER walk failed:', err)
    }

    const userNodes = userResult?.nodes || []
    const hasUserNode = userNodes.some(n => n.name === 'USER')
    const galaxyCenters = userNodes.filter(n => n.name !== 'USER')

    if (hasUserNode && galaxyCenters.length > 0) {
      // ── Galaxies form around USER's 1-hop neighbours ──────────────
      // USER + its direct edges → galaxy 0 (central cluster)
      addNode({ node_id: 0, name: 'USER', entity_type: 'Person', description: '', confidence: 1.0, scope: 'global' } as KgNode, 0)
      if (userResult) addResult(userResult, 0)

      // Walk each galaxy centre (up to 6) in parallel
      const centers = galaxyCenters.slice(0, 6)
      const galaxyResults = await Promise.all(
        centers.map(center =>
          loomRpc<KgGraph>('kg.walk', {
            start_name: center.name, max_depth: maxDepth, scope, limit: 60,
          }).catch(err => {
            console.error('[kgLoadGraph] galaxy walk failed for:', center.name, err)
            return null
          })
        )
      )
      for (let gi = 0; gi < galaxyResults.length; gi++) {
        const result = galaxyResults[gi]
        if (result) addResult(result, gi + 1)
      }

      // ── Phase 3: Drop cross-galaxy edges ──────────────────────────
      const galaxyEdges: KgEdge[] = []
      for (const e of edgeMap.values()) {
        const srcG = nodeGalaxy.get(e.source)
        const tgtG = nodeGalaxy.get(e.target)
        if (srcG === undefined || tgtG === undefined) {
          galaxyEdges.push(e) // keep (shouldn't happen)
        } else if (srcG === tgtG) {
          galaxyEdges.push(e) // same galaxy — keep
        } else if (e.source === 'USER' || e.target === 'USER') {
          galaxyEdges.push(e) // USER ↔ galaxy — keep
        }
        // else: cross-galaxy edge → silently dropped
      }

      // Add orphan nodes from kgNodeList as distant stars
      if (sourceNodes.length > nodeMap.size) {
        for (const n of sourceNodes) {
          if (!nodeMap.has(n.name)) addNode(n, 999)
        }
      }

      if (generation === kgGraphGeneration) {
        set({
          kgGraph: { nodes: [...nodeMap.values()], edges: galaxyEdges },
          kgSelectedNode: null,
        })
      }
    } else {
      // ── Fallback: no USER neighbours → walk from seed list in parallel
      const walkResults = await Promise.all(
        seeds.map(name => {
          const depth = name === 'USER' ? 1 : maxDepth
          return loomRpc<KgGraph>('kg.walk', {
            start_name: name, max_depth: depth, scope, limit: 100,
          }).then(result => ({ name, result })).catch(err => {
            console.error('[kgLoadGraph] walk failed for seed:', name, err)
            return { name, result: null as KgGraph | null }
          })
        })
      )
      for (const { name, result } of walkResults) {
        if (result) addResult(result, name === 'USER' ? 0 : 99)
      }

      if (nodeMap.size === 0 || sourceNodes.length > nodeMap.size) {
        for (const n of sourceNodes) {
          if (!nodeMap.has(n.name)) addNode(n, 999)
        }
      }

      if (generation === kgGraphGeneration) {
        set({
          kgGraph: { nodes: [...nodeMap.values()], edges: [...edgeMap.values()] },
          kgSelectedNode: null,
        })
      }
    }
  },

  kgLoadStats: async () => {
    try {
      const result = await loomRpc<KgStats>('kg.stats')
      set({ kgStats: result })
    } catch (err) {
      console.error('[kgLoadStats] failed:', err)
      ;(get() as any).addToast?.({ type: 'error', message: _t('kg.loadFailed') })
    }
  },

  kgClearGraph: () => {
    kgGraphGeneration++
    set({ kgGraph: null, kgSelectedNode: null })
  },

  kgListNodes: async (scope) => {
    const generation = ++kgListGeneration
    try {
      const nodes = await loadAllKgNodes(async (limit, offset) => {
        const result = await loomRpc<{ nodes: KgNode[] }>('kg.list', { limit, offset, scope })
        return result.nodes ?? []
      })
      if (generation === kgListGeneration) set({ kgNodeList: nodes })
      return nodes
    } catch (err) {
      console.error('[kgListNodes] failed:', err)
      ;(get() as any).addToast?.({ type: 'error', message: _t('kg.loadFailed') })
      return []
    }
  },

  kgNodeDelete: async (name) => {
    await loomRpc('kg.node.delete', { name })
    set(s => ({
      kgNodeList: s.kgNodeList.filter(n => n.name !== name),
      kgGraph: s.kgGraph ? {
        nodes: s.kgGraph.nodes.filter(n => n.name !== name),
        edges: s.kgGraph.edges.filter(e => e.source !== name && e.target !== name),
      } : null,
    }))
    await get().kgLoadStats()
  },

  kgEdgeDelete: async (source, target, relation) => {
    await loomRpc('kg.edge.delete', { source, target, relation })
    set(s => ({
      kgGraph: s.kgGraph ? {
        ...s.kgGraph,
        edges: s.kgGraph.edges.filter(
          e => !(e.source === source && e.target === target && e.relation_type === relation)
        ),
      } : null,
    }))
    await get().kgLoadStats()
  },

  cognitionListBySubject: async (subject, scope) => {
    const result = await loomRpc<{ rows: Cognition[] }>('cognitions.list', {
      subject, scope, limit: 200, offset: 0,
    })
    set({ cognitionList: result.rows ?? [], cognitionPage: 0 })
  },

  cognitionSetPage: (page) => {
    set({ cognitionPage: page })
  },

  cognitionListSubjects: async () => {
    const result = await loomRpc<{ subjects: string[] }>('cognitions.subjects', {})
    set({ cognitionSubjects: result.subjects ?? [] })
  },

  cognitionLoadSnapshots: async (cognitionId) => {
    const result = await loomRpc<{ snapshots: CognitionHistory[] }>('cognitions.snapshots', {
      cognition_id: cognitionId,
    })
    set(s => ({
      cognitionSnapshots: { ...s.cognitionSnapshots, [cognitionId]: result.snapshots ?? [] },
    }))
  },

  cognitionDelete: async (id) => {
    const result = await loomRpc<{ deleted: boolean }>('cognitions.delete', { id })
    if (result.deleted) {
      set(s => ({ cognitionList: s.cognitionList.filter(c => c.id !== id) }))
    }
    return result.deleted
  },

  kgPrune: async (olderThanDays) => {
    await loomRpc('kg.prune', { older_than_days: olderThanDays })
    await get().kgLoadStats()
    await get().kgListNodes()
  },

  memoryPromote: async (sessionId, minConfidence = 0.5) => {
    const result = await loomRpc<{ promoted_nodes: number; promoted_cognitions: number }>(
      'memory.promote',
      { session_id: sessionId, min_confidence: minConfidence },
    )
    await get().kgLoadStats()
    // Refresh cognition list after promotion
    const curSubject = 'USER'
    await get().cognitionListBySubject(curSubject, undefined)
    return result
  },

  kgLoadHealth: async () => {
    try {
      const health = await loomRpc<MemoryHealth>('memory.health')
      set({ memoryHealth: health })
    } catch (err) {
      console.error('[kgLoadHealth] failed:', err)
    }
  },

  kgLoadQuality: async (lookbackDays) => {
    try {
      const report = await loomRpc<MemoryQualityReport>('memory.quality', { lookback_days: lookbackDays ?? 30 })
      set({ qualityReport: report })
    } catch (err) {
      console.error('[kgLoadQuality] failed:', err)
    }
  },

  kgLoadPersona: async () => {
    try {
      const result = await loomRpc<PersonaData>('memory.persona')
      set({ personaData: result })
    } catch (err) {
      console.error('[kgLoadPersona] failed:', err)
    }
  },

  kgLoadPatterns: async () => {
    try {
      const report = await loomRpc<SessionPatternReport>('memory.patterns')
      set({ patternReport: report })
    } catch (err) {
      console.error('[kgLoadPatterns] failed:', err)
    }
  },

  kgRunConsolidation: async () => {
    try {
      const report = await loomRpc<ConsolidationReport>('memory.consolidate')
      set({ consolidationReport: report })
    } catch (err) {
      console.error('[kgRunConsolidation] failed:', err)
      throw err
    }
  },

  kgRunForgetting: async (minImportance, maxAgeDays) => {
    try {
      const report = await loomRpc<ForgettingReport>('memory.forget', {
        min_importance: minImportance ?? 0.3,
        max_age_days: maxAgeDays ?? 90,
      })
      set({ forgettingReport: report })
    } catch (err) {
      console.error('[kgRunForgetting] failed:', err)
      throw err
    }
  },

  kgLoadPipelineStatus: async () => {
    try {
      const status = await loomRpc<PipelineStatus>('memory.pipeline_status')
      set({ pipelineStatus: status ? [status] : [] })
    } catch (err) {
      console.error('[kgLoadPipelineStatus] failed:', err)
    }
  },

  kgLoadLayerStats: async () => {
    try {
      const result = await loomRpc<{ layers: [string, number][] }>('memory.layer_stats')
      const stats = (result.layers ?? []).map(([layer_name, node_count]) => ({ layer_name, node_count }))
      set({ layerStats: stats })
    } catch (err) {
      console.error('[kgLoadLayerStats] failed:', err)
    }
  },

  kgPromoteToLayer: async (nodeName, targetLayer) => {
    try {
      await loomRpc('memory.promote_to_layer', { node_name: nodeName, target_layer: targetLayer })
      // Refresh stats and node list
      await get().kgLoadStats()
      await get().kgLoadLayerStats()
      const nodes = get().kgNodeList
      set({
        kgNodeList: nodes.map(n =>
          n.name === nodeName ? { ...n, layer: targetLayer } : n
        ),
      })
    } catch (err) {
      console.error('[kgPromoteToLayer] failed:', err)
      throw err
    }
  },

  kgSetActiveTab: (tab) => {
    set({ activeTab: tab })
  },
})
