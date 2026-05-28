import { useState, useEffect, useRef, useCallback } from 'react'
import ForceGraph2D from 'react-force-graph-2d'
import { useStore } from '../../stores'
import type { KgNode } from '../../types/bindings'
import styles from './KnowledgeGraphPanel.module.css'

const ENTITY_COLORS: Record<string, string> = {
  Person: '#22d3ee',
  Technology: '#a78bfa',
  Topic: '#fb923c',
  Project: '#34d399',
  Interest: '#f472b6',
  Skill: '#fbbf24',
  Tool: '#818cf8',
  Language: '#2dd4bf',
  Framework: '#c084fc',
}
const DEFAULT_COLOR = '#9ca3af'

function entityColor(type: string): string {
  return ENTITY_COLORS[type] ?? DEFAULT_COLOR
}

interface GraphNode {
  id: string
  node_id: number
  name: string
  entity_type: string
  description: string
  confidence: number
  color: string
  val: number
}

interface GraphLink {
  source: string
  target: string
  relation_type: string
  confidence: number
}

export default function KnowledgeGraphPanel() {
  const kgStats = useStore(s => s.kgStats)
  const kgSearchResults = useStore(s => s.kgSearchResults)
  const kgGraph = useStore(s => s.kgGraph)
  const kgNodeList = useStore(s => s.kgNodeList)
  const kgSearch = useStore(s => s.kgSearch)
  const kgExpandNode = useStore(s => s.kgExpandNode)
  const kgWalkFrom = useStore(s => s.kgWalkFrom)
  const kgLoadStats = useStore(s => s.kgLoadStats)
  const kgClearGraph = useStore(s => s.kgClearGraph)
  const kgListNodes = useStore(s => s.kgListNodes)
  const kgNodeDelete = useStore(s => s.kgNodeDelete)
  const kgEdgeDelete = useStore(s => s.kgEdgeDelete)
  const showConfirm = useStore(s => s.showConfirm)

  const [query, setQuery] = useState('')
  const [tooltip, setTooltip] = useState<{ node: GraphNode; x: number; y: number } | null>(null)
  const [activeTab, setActiveTab] = useState<'list' | 'graph'>('list')
  const fgRef = useRef<any>(null)
  const wrapRef = useRef<HTMLDivElement>(null)
  const [dimensions, setDimensions] = useState({ w: 600, h: 400 })

  useEffect(() => { kgLoadStats(); kgListNodes() }, [kgLoadStats, kgListNodes])

  // Auto-populate graph when switching to graph tab with no data
  useEffect(() => {
    if (activeTab === 'graph' && !kgGraph && kgNodeList.length > 0) {
      kgWalkFrom(kgNodeList[0].name, 2)
    }
  }, [activeTab, kgGraph, kgNodeList, kgWalkFrom])

  useEffect(() => {
    const el = wrapRef.current
    if (!el) return
    const ro = new ResizeObserver(entries => {
      for (const e of entries) {
        setDimensions({ w: e.contentRect.width, h: Math.max(e.contentRect.height, 300) })
      }
    })
    ro.observe(el)
    return () => ro.disconnect()
  }, [])

  // Convert KgGraph to react-force-graph format
  const graphData = (() => {
    if (!kgGraph || kgGraph.nodes.length === 0) return { nodes: [], links: [] }

    const nodeMap = new Map<string, GraphNode>()
    for (const n of kgGraph.nodes) {
      nodeMap.set(n.name, {
        id: n.name,
        node_id: n.node_id,
        name: n.name,
        entity_type: n.entity_type,
        description: n.description,
        confidence: n.confidence,
        color: entityColor(n.entity_type),
        val: 3 + n.confidence * 8, // node size
      })
    }

    const links: GraphLink[] = []
    for (const e of kgGraph.edges) {
      if (nodeMap.has(e.source) && nodeMap.has(e.target)) {
        links.push({
          source: e.source,
          target: e.target,
          relation_type: e.relation_type,
          confidence: e.confidence,
        })
      }
    }

    return { nodes: [...nodeMap.values()], links }
  })()

  const handleSearch = useCallback(() => {
    if (!query.trim()) return
    kgSearch(query.trim())
  }, [query, kgSearch])

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleSearch()
  }, [handleSearch])

  const handleExpand = useCallback((name: string) => {
    setTooltip(null)
    kgExpandNode(name)
  }, [kgExpandNode])

  const handleWalk = useCallback((name: string) => {
    setTooltip(null)
    kgWalkFrom(name)
  }, [kgWalkFrom])

  // Canvas paint for nodes
  const paintNode = useCallback((node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
    const n = node as GraphNode
    const r = n.val / globalScale
    const fontSize = Math.max(10, 12 / globalScale)

    // Glow
    ctx.beginPath()
    ctx.arc(n.x!, n.y!, r + 3, 0, Math.PI * 2)
    ctx.fillStyle = n.color + '22'
    ctx.fill()

    // Node circle
    ctx.beginPath()
    ctx.arc(n.x!, n.y!, r, 0, Math.PI * 2)
    ctx.fillStyle = n.color + 'cc'
    ctx.fill()
    ctx.strokeStyle = 'rgba(255,255,255,0.25)'
    ctx.lineWidth = 0.5 / globalScale
    ctx.stroke()

    // Label
    const label = n.name.length > 12 ? n.name.slice(0, 12) + '...' : n.name
    ctx.font = `${fontSize}px -apple-system, sans-serif`
    ctx.fillStyle = 'rgba(255,255,255,0.7)'
    ctx.textAlign = 'center'
    ctx.fillText(label, n.x!, n.y! + r + fontSize + 2)
  }, [])

  // Canvas paint for link labels
  const paintLinkLabel = useCallback((link: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
    const l = link as GraphLink
    if (!l.relation_type) return
    const mx = (link.source.x + link.target.x) / 2
    const my = (link.source.y + link.target.y) / 2
    const fontSize = Math.max(8, 10 / globalScale)
    ctx.font = `${fontSize}px -apple-system, sans-serif`
    ctx.fillStyle = 'rgba(255,255,255,0.3)'
    ctx.textAlign = 'center'
    ctx.fillText(l.relation_type, mx, my - 4)
  }, [])

  const handleNodeClick = useCallback((node: any, event: MouseEvent) => {
    const n = node as GraphNode
    const rect = wrapRef.current?.getBoundingClientRect()
    if (rect) {
      setTooltip(prev => prev?.node.id === n.id ? null : {
        node: n,
        x: event.clientX - rect.left + 10,
        y: event.clientY - rect.top - 10,
      })
    }
  }, [])

  const handleDeleteNode = useCallback(async (name: string) => {
    const ok = await showConfirm('删除实体', `确定删除 "${name}" 及其所有关系？`, true)
    if (!ok) return
    kgNodeDelete(name)
  }, [kgNodeDelete, showConfirm])

  const handleDeleteEdge = useCallback(async (source: string, target: string, relation: string) => {
    kgEdgeDelete(source, target, relation)
  }, [kgEdgeDelete])

  const hasData = graphData.nodes.length > 0
  // search results overlay on top of node list; clear when query is empty
  const displayNodes = query && kgSearchResults.length > 0 ? kgSearchResults : kgNodeList

  return (
    <div className={styles.panel}>
      <div className={styles.searchRow}>
        <input
          className={styles.searchInput}
          value={query}
          onChange={e => {
            setQuery(e.target.value)
            if (!e.target.value) setActiveTab('list')
          }}
          onKeyDown={handleKeyDown}
          placeholder="筛选实体..."
        />
        <button className={styles.searchBtn} onClick={handleSearch}>搜索</button>
      </div>

      {kgStats && (
        <div className={styles.stats}>
          <span>实体 <span className={styles.statValue}>{kgStats.node_count}</span></span>
          <span>关系 <span className={styles.statValue}>{kgStats.edge_count}</span></span>
        </div>
      )}

      {/* Tabs */}
      <div className={styles.tabs}>
        <button
          className={`${styles.tab} ${activeTab === 'list' ? styles.tabActive : ''}`}
          onClick={() => setActiveTab('list')}
        >实体列表</button>
        <button
          className={`${styles.tab} ${activeTab === 'graph' ? styles.tabActive : ''}`}
          onClick={() => setActiveTab('graph')}
        >图谱视图</button>
        {kgGraph && (
          <button className={styles.clearBtn} onClick={kgClearGraph} style={{ marginLeft: 'auto' }}>清除图谱</button>
        )}
      </div>

      {activeTab === 'list' && (
        <div className={styles.nodeList}>
          {displayNodes.length === 0 ? (
            <div className={styles.canvasEmpty}>
              暂无实体。认知图谱会在与 AI 对话过程中自动积累。
            </div>
          ) : (
            displayNodes.map(n => (
              <div key={n.node_id || n.name} className={styles.nodeItem}>
                <div className={styles.nodeItemInfo}>
                  <span className={styles.nodeItemName}>{n.name}</span>
                  <span
                    className={styles.nodeItemType}
                    style={{ color: entityColor(n.entity_type), background: entityColor(n.entity_type) + '18' }}
                  >{n.entity_type}</span>
                  <span className={styles.nodeItemConf}>{(n.confidence * 100).toFixed(0)}%</span>
                </div>
                {n.description && (
                  <div className={styles.nodeItemDesc}>{n.description}</div>
                )}
                <div className={styles.nodeItemActions}>
                  <button className={styles.actionBtn} onClick={() => handleExpand(n.name)}>邻居</button>
                  <button className={styles.actionBtn} onClick={() => handleWalk(n.name)}>遍历</button>
                  <button className={styles.actionBtnDanger} onClick={() => handleDeleteNode(n.name)}>删除</button>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {activeTab === 'graph' && (
        <>
          <div className={styles.canvasWrap} ref={wrapRef}>
            {!hasData ? (
              <div className={styles.canvasEmpty}>
                {kgNodeList.length > 0 ? '正在加载图谱...' : '暂无实体数据'}
              </div>
            ) : (
              <ForceGraph2D
                ref={fgRef}
                graphData={graphData}
                width={dimensions.w}
                height={dimensions.h}
                nodeCanvasObject={paintNode}
                linkCanvasObject={paintLinkLabel}
                linkDirectionalArrowLength={3}
                linkDirectionalArrowRelPos={1}
                linkWidth={0.5}
                linkColor={() => 'rgba(255,255,255,0.12)'}
                onNodeClick={handleNodeClick}
                cooldownTicks={100}
                d3AlphaDecay={0.02}
                d3VelocityDecay={0.3}
                enableZoomInteraction
                enablePanInteraction
                minZoom={0.3}
                maxZoom={4}
              />
            )}

            {tooltip && (
              <div
                className={styles.tooltip}
                style={{
                  left: Math.min(tooltip.x, (wrapRef.current?.clientWidth ?? 600) - 270),
                  top: Math.min(tooltip.y, (wrapRef.current?.clientHeight ?? 400) - 180),
                }}
              >
                <div className={styles.tooltipName}>{tooltip.node.name}</div>
                <div className={styles.tooltipType}>{tooltip.node.entity_type}</div>
                {tooltip.node.description && (
                  <div className={styles.tooltipDesc}>{tooltip.node.description}</div>
                )}
                <div className={styles.tooltipConf}>
                  置信度 {(tooltip.node.confidence * 100).toFixed(0)}%
                </div>
                <div className={styles.tooltipActions}>
                  <button className={styles.tooltipBtn} onClick={() => handleExpand(tooltip.node.name)}>
                    展开邻居
                  </button>
                  <button className={styles.tooltipBtn} onClick={() => handleWalk(tooltip.node.name)}>
                    BFS 遍历
                  </button>
                  <button className={styles.tooltipBtn} onClick={() => setTooltip(null)}>
                    关闭
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* Edge list */}
          {hasData && (
            <div className={styles.edgeList}>
              <div className={styles.edgeListTitle}>关系列表</div>
              {graphData.links.map((l, i) => (
                <div key={i} className={styles.edgeItem}>
                  <span className={styles.edgeLabel}>
                    <span className={styles.edgeNode}>{l.source as string}</span>
                    <span className={styles.edgeRel}>{l.relation_type || 'related_to'}</span>
                    <span className={styles.edgeNode}>{l.target as string}</span>
                  </span>
                  <button
                    className={styles.edgeDelBtn}
                    onClick={() => handleDeleteEdge(l.source as string, l.target as string, l.relation_type)}
                    title="删除关系"
                  >x</button>
                </div>
              ))}
            </div>
          )}
        </>
      )}

      <div className={styles.legend}>
        {Object.entries(ENTITY_COLORS).slice(0, 8).map(([type, color]) => (
          <div key={type} className={styles.legendItem}>
            <span className={styles.legendDot} style={{ background: color }} />
            {type}
          </div>
        ))}
      </div>
    </div>
  )
}
