import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import GalaxyGraph from './GalaxyGraph'
import { useStore } from '../../stores'
import Select from '../shared/Select'
import type { KgNode } from '../../types/bindings'
import StarField from './StarField'
import styles from './KnowledgeGraphPanel.module.css'

// Hash entity type → stable hue, so each type gets a distinct color.
function hashHue(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = ((h << 5) - h + s.charCodeAt(i)) | 0
  return Math.abs(h) % 360
}

function entityColor(type: string): string {
  return `hsl(${hashHue(type)}, 70%, 62%)`
}

const ENTITY_TYPE_CN: Record<string, string> = {
  Technology: '技术', Person: '人物', Project: '项目', Concept: '概念',
  Tool: '工具', Topic: '话题', Organization: '组织',
}

function entityTypeCn(type: string): string {
  return ENTITY_TYPE_CN[type] ?? type
}

const PAGE_SIZE = 20

const RELATION_LABELS: Record<string, string> = {
  uses: '使用', works_on: '参与', knows: '了解',
  interested_in: '感兴趣', dislikes: '不喜欢', depends_on: '依赖',
  part_of: '属于', created_by: '创建者', related_to: '相关',
}

function translateRelation(rel: string): string {
  return RELATION_LABELS[rel] ?? rel.replace(/_/g, ' ')
}

interface GraphNode {
  id: string
  node_id: number
  name: string
  entity_type: string
  description: string
  confidence: number
  scope: string
  color: string
}

export default function KnowledgeGraphTab({ initialSubTab = 'list' }: { initialSubTab?: 'list' | 'graph' }) {
  const kgStats = useStore(s => s.kgStats)
  const kgSearchResults = useStore(s => s.kgSearchResults)
  const kgGraph = useStore(s => s.kgGraph)
  const kgNodeList = useStore(s => s.kgNodeList)
  const kgSearch = useStore(s => s.kgSearch)
  const kgExpandNode = useStore(s => s.kgExpandNode)
  const kgWalkFrom = useStore(s => s.kgWalkFrom)
  const kgLoadGraph = useStore(s => s.kgLoadGraph)
  const kgLoadStats = useStore(s => s.kgLoadStats)
  const kgListNodes = useStore(s => s.kgListNodes)
  const kgNodeDelete = useStore(s => s.kgNodeDelete)
  const kgEdgeDelete = useStore(s => s.kgEdgeDelete)
  const showConfirm = useStore(s => s.showConfirm)
  const currentSessionId = useStore(s => s.currentSessionId)

  const [query, setQuery] = useState('')
  const [scopeFilter, setScopeFilter] = useState<'all' | 'global' | 'session'>('all')
  const [tooltip, setTooltip] = useState<{ node: GraphNode; x: number; y: number } | null>(null)
  const [activeTab] = useState<'list' | 'graph'>(initialSubTab)
  const [isFullscreen, setIsFullscreen] = useState(false)
  const [showLabels, setShowLabels] = useState(true)
  const [listPage, setListPage] = useState(0)

  const chartWrapRef = useRef<HTMLDivElement>(null)
  const fullscreenWrapRef = useRef<HTMLDivElement>(null)
  const [chartSize, setChartSize] = useState({ w: 0, h: 0 })

  /** Suppresses auto-populate after user intentionally clears the graph */
  const userClearedGraph = useRef(false)

  useEffect(() => {
    kgLoadStats()
    const effectiveScope = scopeFilter === 'all' ? undefined
      : scopeFilter === 'session' ? (currentSessionId ?? undefined)
      : scopeFilter
    kgListNodes(effectiveScope)
  }, [kgLoadStats, kgListNodes, scopeFilter, currentSessionId])

  // Auto-populate graph when switching to graph tab with no data.
  // Walk from the first few listed nodes in parallel and merge results,
  // so all connected components and their edges appear.
  // Skip if user intentionally cleared the graph.
  useEffect(() => {
    if (activeTab !== 'graph') {
      // Reset the clear-flag when user leaves the graph tab,
      // so auto-populate works again when they return.
      userClearedGraph.current = false
      return
    }
    if (!kgGraph && !userClearedGraph.current) {
      // Always include USER as the central seed, then pick distinct
      // seeds from across the list so all galaxies get discovered.
      const list = kgSearchResults.length > 0 ? kgSearchResults : kgNodeList
      const seen = new Set(['USER'])
      const extras: string[] = []
      for (const n of list) {
        if (!seen.has(n.name)) {
          seen.add(n.name)
          extras.push(n.name)
          if (extras.length >= 4) break
        }
      }
      const seeds = ['USER', ...extras]

      if (seeds.length > 0) {
        const effectiveScope = scopeFilter === 'all' ? undefined
          : scopeFilter === 'session' ? (currentSessionId ?? undefined)
          : scopeFilter
        console.log('[KnowledgeGraphTab] Auto-populating graph from seeds:', seeds, 'scope:', effectiveScope)
        kgLoadGraph(seeds, 2, effectiveScope)
      } else {
        console.log('[KnowledgeGraphTab] No data available to populate graph')
      }
    }
  }, [activeTab, kgGraph, kgNodeList, kgSearchResults, kgLoadGraph, scopeFilter, currentSessionId])

  // Debug: log when graph data changes
  useEffect(() => {
    if (activeTab === 'graph') {
      console.log('[KnowledgeGraphTab] Graph data changed:', {
        hasKgGraph: !!kgGraph,
        nodeCount: kgGraph?.nodes.length || 0,
        edgeCount: kgGraph?.edges.length || 0,
        hasData: !!kgGraph && kgGraph.nodes.length > 0,
        nodes: kgGraph?.nodes.map(n => n.name),
        edges: kgGraph?.edges.map(e => `${e.source} -> ${e.target}`),
      })
    }
  }, [activeTab, kgGraph])

  // Track canvas container size for StarField background
  useEffect(() => {
    const el = chartWrapRef.current
    if (!el || activeTab !== 'graph') return
    const measure = () => {
      const rect = el.getBoundingClientRect()
      if (rect.width > 0 && rect.height > 0) setChartSize({ w: Math.ceil(rect.width), h: Math.ceil(rect.height) })
    }
    measure()
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    return () => ro.disconnect()
  }, [activeTab])

  const handleSearch = useCallback(() => {
    if (!query.trim()) return
    kgSearch(query.trim())
  }, [query, kgSearch])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter') handleSearch()
    },
    [handleSearch],
  )

  const handleExpand = useCallback(
    (name: string) => {
      userClearedGraph.current = false
      setTooltip(null)
      const effectiveScope = scopeFilter === 'all' ? undefined
        : scopeFilter === 'session' ? (currentSessionId ?? undefined)
        : scopeFilter
      kgExpandNode(name, effectiveScope)
    },
    [kgExpandNode, scopeFilter, currentSessionId],
  )

  const handleWalk = useCallback(
    (name: string) => {
      userClearedGraph.current = false
      setTooltip(null)
      const effectiveScope = scopeFilter === 'all' ? undefined
        : scopeFilter === 'session' ? (currentSessionId ?? undefined)
        : scopeFilter
      kgWalkFrom(name, 2, effectiveScope)
    },
    [kgWalkFrom, scopeFilter, currentSessionId],
  )

  const handleDeleteNode = useCallback(
    async (name: string) => {
      const ok = await showConfirm('删除实体', `确定删除 "${name}" 及其所有关系？`, true)
      if (!ok) return
      kgNodeDelete(name)
    },
    [kgNodeDelete, showConfirm],
  )

  const handleRefreshGraph = useCallback(() => {
    userClearedGraph.current = false
    const list = kgSearchResults.length > 0 ? kgSearchResults : kgNodeList
    const seen = new Set(['USER'])
    const extras: string[] = []
    for (const n of list) {
      if (!seen.has(n.name)) {
        seen.add(n.name)
        extras.push(n.name)
        if (extras.length >= 4) break
      }
    }
    const seeds = ['USER', ...extras]
    const effectiveScope = scopeFilter === 'all' ? undefined
      : scopeFilter === 'session' ? (currentSessionId ?? undefined)
      : scopeFilter
    kgLoadGraph(seeds, 2, effectiveScope)
  }, [kgSearchResults, kgNodeList, kgLoadGraph, scopeFilter, currentSessionId])

  const handleDeleteEdge = useCallback(
    async (source: string, target: string, relation: string) => {
      kgEdgeDelete(source, target, relation)
    },
    [kgEdgeDelete],
  )

  const hasData = !!(kgGraph && kgGraph.nodes.length > 0)
  // search results overlay on top of node list; clear when query is empty
  const displayNodes = query && kgSearchResults.length > 0 ? kgSearchResults : kgNodeList

  // ── Pagination ──
  const totalPages = Math.max(1, Math.ceil(displayNodes.length / PAGE_SIZE))
  const safePage = Math.min(listPage, totalPages - 1)
  const pagedNodes = useMemo(() => {
    const start = safePage * PAGE_SIZE
    return displayNodes.slice(start, start + PAGE_SIZE)
  }, [displayNodes, safePage])

  // Reset page when data changes
  useEffect(() => { setListPage(0) }, [displayNodes.length])

  return (
    <div className={styles.panel}>
      {/* Search bar + stats — list view only */}
      {initialSubTab === 'list' && (
        <div className={styles.stickyBar}>
          <div className={styles.searchRow}>
            <input
              className={styles.searchInput}
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="筛选实体..."
            />
            <Select
              value={scopeFilter}
              options={[
                { value: 'all', label: '全部范围' },
                { value: 'global', label: '全局' },
                { value: 'session', label: '会话级' },
              ]}
              onChange={v => setScopeFilter(v as 'all' | 'global' | 'session')}
              variant="form"
            />
            <button className={styles.searchBtn} onClick={handleSearch}>
              搜索
            </button>
          </div>
          {kgStats && (
            <div className={styles.stats}>
              <span>实体 <span className={styles.statValue}>{kgStats.node_count}</span></span>
              <span>关系 <span className={styles.statValue}>{kgStats.edge_count}</span></span>
            </div>
          )}
        </div>
      )}

      {/* Tabs — graph view toolbar */}
      {initialSubTab === 'graph' && hasData && (
        <div className={styles.tabs}>
          <button className={styles.labelToggleBtn} onClick={() => setShowLabels(v => !v)} title={showLabels ? '隐藏标签' : '显示标签'}>
            {showLabels ? 'Aa' : 'Aa'}
          </button>
          <button className={styles.fullscreenBtn} onClick={() => setIsFullscreen(!isFullscreen)} title={isFullscreen ? '退出全屏' : '全屏查看'}>
            {isFullscreen ? '⤓' : '⤢'}
          </button>
          {kgGraph && (
            <button className={styles.clearBtn} onClick={handleRefreshGraph}>刷新图谱</button>
          )}
          {kgStats && (
            <span className={styles.tabsStats}>
              实体 {kgStats.node_count} · 关系 {kgStats.edge_count}
            </span>
          )}
        </div>
      )}

      {initialSubTab === 'list' && (
        <div className={styles.nodeList}>
          {displayNodes.length === 0 ? (
            <div className={styles.canvasEmpty}>
              暂无实体。知识图谱会在与 AI 对话过程中自动积累。
            </div>
          ) : (
            <>
              {pagedNodes.map(n => (
                <div key={n.node_id || n.name} className={styles.nodeItem}>
                  <div className={styles.nodeItemInfo}>
                    <div className={styles.nodeItemMeta}>
                      <span className={styles.nodeItemName}>{n.name}</span>
                      <span
                        className={styles.nodeItemType}
                        style={{
                          color: entityColor(n.entity_type),
                          background: entityColor(n.entity_type) + '18',
                        }}
                      >
                        {entityTypeCn(n.entity_type)}
                      </span>
                      {n.scope && n.scope !== 'global' && (
                        <span className={styles.scopeBadge}>{n.scope.slice(0, 6)}</span>
                      )}
                      <span className={styles.nodeItemConf} title="AI 对该实体信息的把握程度">
                        确信 {(n.confidence * 100).toFixed(0)}%
                      </span>
                    </div>
                    <div className={styles.nodeItemActions}>
                      <button className={styles.actionBtn} onClick={() => handleExpand(n.name)}>关联</button>
                      <button className={styles.actionBtn} onClick={() => handleWalk(n.name)}>关系网</button>
                      <span className={styles.actionDivider} />
                      <button className={styles.actionBtnDanger} onClick={() => handleDeleteNode(n.name)}>删除</button>
                    </div>
                  </div>
                  {n.description && <div className={styles.nodeItemDesc}>{n.description}</div>}
                </div>
              ))}
              {/* Pagination */}
              {totalPages > 1 && (
                <div className={styles.pagination}>
                  <button className={styles.pageBtn} disabled={safePage === 0} onClick={() => setListPage(p => Math.max(0, p - 1))}>上一页</button>
                  <span className={styles.pageInfo}>
                    {safePage + 1} / {totalPages}
                    <span className={styles.pageTotal}>（共 {displayNodes.length} 条）</span>
                  </span>
                  <button className={styles.pageBtn} disabled={safePage >= totalPages - 1} onClick={() => setListPage(p => Math.min(totalPages - 1, p + 1))}>下一页</button>
                </div>
              )}
            </>
          )}
        </div>
      )}

      {initialSubTab === 'graph' && (
        <>
          <div className={styles.canvasWrap} ref={chartWrapRef}>
            {chartSize.w > 0 && <StarField width={chartSize.w} height={chartSize.h} className={styles.starField} />}
            {!hasData ? (
              <div className={styles.canvasEmpty}>
                <span>{kgNodeList.length > 0 ? '正在加载图谱...' : '星图暂无数据'}</span>
                {kgNodeList.length === 0 && (
                  <span className={styles.canvasEmptyHint}>
                    在与 AI 对话过程中，知识图谱会自动积累实体与关系，构建属于你的认知网络
                  </span>
                )}
              </div>
            ) : (
              <GalaxyGraph
                nodes={kgGraph.nodes}
                edges={kgGraph.edges}
                showLabels={showLabels}
                onNodeClick={(node, cx, cy) => setTooltip({ node, x: cx, y: cy })}
                onBackgroundClick={() => setTooltip(null)}
              />
            )}
          </div>

          {/* Edge list */}
          {hasData && kgGraph && (
            <div className={styles.edgeList}>
              <div className={styles.edgeListTitle}>关系列表 ({kgGraph.edges.length})</div>
              {kgGraph.edges.map((e, i) => {
                const srcName = e.source
                const tgtName = e.target
                return (
                  <div key={i} className={styles.edgeItem}>
                    <span className={styles.edgeNode}>{srcName}</span>
                    <span className={styles.edgeRel}>{translateRelation(e.relation_type) || '相关'}</span>
                    <span className={styles.edgeNode}>{tgtName}</span>
                    <button
                      className={styles.edgeDelBtn}
                      onClick={() =>
                        handleDeleteEdge(srcName, tgtName, e.relation_type)
                      }
                      title="删除关系"
                    >
                      x
                    </button>
                  </div>
                )
              })}
            </div>
          )}
        </>
      )}

      <div className={styles.legend}>
        {(() => {
          const types = [...new Set(kgGraph ? kgGraph.nodes.map(n => n.entity_type) : [])]
          return types.map(type => (
            <div key={type} className={styles.legendItem}>
              <span className={styles.legendDot} style={{ background: entityColor(type) }} />
              {entityTypeCn(type)}
            </div>
          ))
        })()}
      </div>

      {/* Tooltip popup — single instance rendered at root level with fixed positioning */}
      {tooltip && (
        <div
          className={styles.tooltip}
          style={{
            position: 'fixed',
            left: tooltip.x + 12,
            top: tooltip.y - 12,
            zIndex: 10001,
          }}
        >
          <div className={styles.tooltipName}>{tooltip.node.name}</div>
          <div className={styles.tooltipType}>{entityTypeCn(tooltip.node.entity_type)}</div>
          {tooltip.node.description && (
            <div className={styles.tooltipDesc}>{tooltip.node.description}</div>
          )}
          <div className={styles.tooltipConf}>
            确信度 {(tooltip.node.confidence * 100).toFixed(0)}%
          </div>
          <div className={styles.tooltipActions}>
            <button className={styles.tooltipBtn} onClick={() => handleExpand(tooltip.node.name)}>
              查看关联
            </button>
            <button className={styles.tooltipBtn} onClick={() => handleWalk(tooltip.node.name)}>
              展开关系网
            </button>
            <button className={styles.tooltipBtn} onClick={() => setTooltip(null)}>
              关闭
            </button>
          </div>
        </div>
      )}

      {/* Fullscreen portal */}
      {isFullscreen && activeTab === 'graph' && hasData && (
        <div className={styles.fullscreenOverlay} ref={fullscreenWrapRef} style={{ display: 'block' }}>
          <StarField width={window.innerWidth} height={window.innerHeight} className={styles.starField} />
          <GalaxyGraph
            nodes={kgGraph.nodes}
            edges={kgGraph.edges}
            showLabels={showLabels}
            onNodeClick={(node, cx, cy) => setTooltip({ node, x: cx, y: cy })}
            onBackgroundClick={() => setTooltip(null)}
          />
          <button
            className={styles.fullscreenLabelBtn}
            onClick={() => setShowLabels(v => !v)}
            title={showLabels ? '隐藏标签' : '显示标签'}
          >
            {showLabels ? 'Aa' : 'Aa'}
          </button>
          <button
            className={styles.fullscreenCloseBtn}
            onClick={() => {
              setTooltip(null)
              setIsFullscreen(false)
            }}
            title="退出全屏"
          >
            ✕
          </button>
        </div>
      )}
    </div>
  )
}
