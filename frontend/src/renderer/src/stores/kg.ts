import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'
import type { KgNode, KgEdge, KgGraph, KgStats, Cognition, CognitionHistory } from '../types/bindings'

export interface KgSlice {
  kgSearchResults: KgNode[]
  kgGraph: KgGraph | null
  kgSelectedNode: KgNode | null
  kgStats: KgStats | null
  kgNodeList: KgNode[]
  cognitionList: Cognition[]
  cognitionSubjects: string[]
  cognitionSnapshots: Record<number, CognitionHistory[]>
  kgSearch: (query: string) => Promise<void>
  kgExpandNode: (nodeName: string) => Promise<void>
  kgWalkFrom: (startName: string, maxDepth?: number, scope?: string) => Promise<void>
  kgLoadGraph: (seeds: string[], maxDepth?: number, scope?: string) => Promise<void>
  kgLoadStats: () => Promise<void>
  kgClearGraph: () => void
  kgListNodes: (scope?: string) => Promise<void>
  kgNodeDelete: (name: string) => Promise<void>
  kgEdgeDelete: (source: string, target: string, relation: string) => Promise<void>
  cognitionListBySubject: (subject: string, scope?: string) => Promise<void>
  cognitionListSubjects: () => Promise<void>
  cognitionLoadSnapshots: (cognitionId: number) => Promise<void>
  kgPrune: (olderThanDays: number) => Promise<void>
}

export const createKgSlice: StateCreator<KgSlice> = (set, get) => ({
  kgSearchResults: [],
  kgGraph: null,
  kgSelectedNode: null,
  kgStats: null,
  kgNodeList: [],
  cognitionList: [],
  cognitionSubjects: [],
  cognitionSnapshots: {},

  kgSearch: async (query) => {
    const result = await loomRpc<{ rows: KgNode[] }>('kg.search', { query, limit: 20 })
    set({ kgSearchResults: result.rows ?? [] })
  },

  kgExpandNode: async (nodeName) => {
    set({ kgSelectedNode: null })
    const result = await loomRpc<KgGraph>('kg.neighbors', { node_name: nodeName, limit: 30 })
    const graph = result as KgGraph
    if (!graph.nodes?.length && !graph.edges?.length) return

    const prev = get().kgGraph
    if (!prev) {
      const center: KgNode = (get().kgSearchResults.find(n => n.name === nodeName))
        ?? get().kgNodeList.find(n => n.name === nodeName)
        ?? { node_id: 0, name: nodeName, entity_type: 'Unknown', description: '', confidence: 1.0, scope: 'global' }
      set({ kgGraph: { nodes: [center, ...graph.nodes], edges: graph.edges } })
      return
    }

    const nodeMap = new Map<string, KgNode>()
    for (const n of prev.nodes) nodeMap.set(n.name, n)
    for (const n of graph.nodes) { if (!nodeMap.has(n.name)) nodeMap.set(n.name, n) }

    const edgeKey = (e: KgEdge) => `${e.source}||${e.target}||${e.relation_type}`
    const edgeSet = new Set(prev.edges.map(edgeKey))
    const newEdges = graph.edges.filter(e => !edgeSet.has(edgeKey(e)))

    set({
      kgGraph: {
        nodes: [...nodeMap.values()],
        edges: [...prev.edges, ...newEdges],
      },
    })
  },

  kgWalkFrom: async (startName, maxDepth = 2, scope) => {
    const result = await loomRpc<KgGraph>('kg.walk', { start_name: startName, max_depth: maxDepth, scope, limit: 50 })
    if (result.nodes?.length) {
      set({ kgGraph: result, kgSelectedNode: null })
    } else {
      // At minimum show the start node itself so the graph isn't stuck empty
      const startNode = get().kgNodeList.find(n => n.name === startName)
      set({
        kgGraph: {
          nodes: startNode ? [startNode] : [{ node_id: 0, name: startName, entity_type: 'Unknown', description: '', confidence: 1.0, scope: 'global' }],
          edges: [],
        },
        kgSelectedNode: null,
      })
    }
  },

  kgLoadGraph: async (seeds, maxDepth = 2, scope) => {
    // Walk from each seed sequentially. Each walk discovers one "galaxy" —
    // a connected component with all its internal edges. Skip seeds already
    // covered by a previous walk's galaxy.
    const nodeMap = new Map<string, KgNode>()
    const edgeMap = new Map<string, KgEdge>()

    const addResult = (r: KgGraph) => {
      for (const n of r.nodes || []) {
        if (!nodeMap.has(n.name)) nodeMap.set(n.name, n)
      }
      for (const e of r.edges || []) {
        const key = `${e.source}||${e.target}||${e.relation_type}`
        if (!edgeMap.has(key)) edgeMap.set(key, e)
      }
    }

    for (const name of seeds) {
      if (nodeMap.has(name)) continue
      try {
        const result = await loomRpc<KgGraph>('kg.walk', { start_name: name, max_depth: maxDepth, scope, limit: 50 })
        addResult(result)
      } catch (err) {
        console.error('[kgLoadGraph] walk failed for seed:', name, err)
      }
    }

    // Add remaining nodes from kgNodeList as single-star galaxies.
    // These are entities with no relationships yet — they show up as
    // isolated stars in the cosmic view, distinct from the connected galaxies.
    if (nodeMap.size === 0 || get().kgNodeList.length > nodeMap.size) {
      for (const n of get().kgNodeList) {
        if (!nodeMap.has(n.name)) nodeMap.set(n.name, n)
      }
    }

    // Fetch ALL edges between loaded nodes to ensure complete connectivity
    // This fills in any edges that walk might have missed due to depth/limit constraints
    if (nodeMap.size > 1) {
      try {
        const nodeNames = Array.from(nodeMap.keys())
        const { edges } = await loomRpc<{ edges: KgEdge[] }>('kg.edges_between', { node_names: nodeNames })
        for (const e of edges || []) {
          const key = `${e.source}||${e.target}||${e.relation_type}`
          if (!edgeMap.has(key)) {
            edgeMap.set(key, e)
          }
        }
      } catch (err) {
        console.error('[kgLoadGraph] edges_between failed:', err)
      }
    }

    set({
      kgGraph: {
        nodes: [...nodeMap.values()],
        edges: [...edgeMap.values()],
      },
      kgSelectedNode: null,
    })
  },

  kgLoadStats: async () => {
    const result = await loomRpc<KgStats>('kg.stats')
    set({ kgStats: result })
  },

  kgClearGraph: () => set({ kgGraph: null, kgSelectedNode: null }),

  kgListNodes: async (scope) => {
    const result = await loomRpc<{ nodes: KgNode[] }>('kg.list', { limit: 50, scope })
    set({ kgNodeList: result.nodes ?? [] })
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
      subject, scope, limit: 50, offset: 0,
    })
    set({ cognitionList: result.rows ?? [] })
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

  kgPrune: async (olderThanDays) => {
    await loomRpc('kg.prune', { older_than_days: olderThanDays })
    await get().kgLoadStats()
    await get().kgListNodes()
  },
})
