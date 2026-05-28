import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'
import type { KgNode, KgEdge, KgGraph, KgStats } from '../types/bindings'

export interface KgSlice {
  kgSearchResults: KgNode[]
  kgGraph: KgGraph | null
  kgSelectedNode: KgNode | null
  kgStats: KgStats | null
  kgNodeList: KgNode[]
  kgSearch: (query: string) => Promise<void>
  kgExpandNode: (nodeName: string) => Promise<void>
  kgWalkFrom: (startName: string, maxDepth?: number) => Promise<void>
  kgLoadStats: () => Promise<void>
  kgClearGraph: () => void
  kgListNodes: () => Promise<void>
  kgNodeDelete: (name: string) => Promise<void>
  kgEdgeDelete: (source: string, target: string, relation: string) => Promise<void>
}

export const createKgSlice: StateCreator<KgSlice> = (set, get) => ({
  kgSearchResults: [],
  kgGraph: null,
  kgSelectedNode: null,
  kgStats: null,
  kgNodeList: [],

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
        ?? { node_id: 0, name: nodeName, entity_type: 'Unknown', description: '', confidence: 1.0 }
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

  kgWalkFrom: async (startName, maxDepth = 2) => {
    const result = await loomRpc<KgGraph>('kg.walk', { start_name: startName, max_depth: maxDepth, limit: 50 })
    if (result.nodes?.length) {
      set({ kgGraph: result, kgSelectedNode: null })
    } else {
      // At minimum show the start node itself so the graph isn't stuck empty
      const startNode = get().kgNodeList.find(n => n.name === startName)
      set({
        kgGraph: {
          nodes: startNode ? [startNode] : [{ node_id: 0, name: startName, entity_type: 'Unknown', description: '', confidence: 1.0 }],
          edges: [],
        },
        kgSelectedNode: null,
      })
    }
  },

  kgLoadStats: async () => {
    const result = await loomRpc<KgStats>('kg.stats')
    set({ kgStats: result })
  },

  kgClearGraph: () => set({ kgGraph: null, kgSearchResults: [], kgSelectedNode: null }),

  kgListNodes: async () => {
    const result = await loomRpc<{ nodes: KgNode[] }>('kg.list', { limit: 50 })
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
  },
})
