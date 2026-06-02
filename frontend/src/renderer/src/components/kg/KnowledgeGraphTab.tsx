import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import ReactEChartsCore from 'echarts-for-react/lib/core'
import * as echarts from 'echarts/core'
import { GraphChart } from 'echarts/charts'
import { TooltipComponent, LegendComponent } from 'echarts/components'
import { CanvasRenderer } from 'echarts/renderers'
import { useStore } from '../../stores'
import Select from '../shared/Select'
import type { KgNode } from '../../types/bindings'
import StarField from './StarField'
import styles from './KnowledgeGraphPanel.module.css'

echarts.use([GraphChart, TooltipComponent, LegendComponent, CanvasRenderer])

const DEFAULT_COLOR = '#9ca3af'

// Hash entity type → stable hue, so each type gets a distinct color.
function hashHue(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = ((h << 5) - h + s.charCodeAt(i)) | 0
  return Math.abs(h) % 360
}

function entityColor(type: string): string {
  return `hsl(${hashHue(type)}, 70%, 62%)`
}

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

interface GraphLink {
  source: string
  target: string
  relation_type: string
  confidence: number
}

export default function KnowledgeGraphTab() {
  const kgStats = useStore(s => s.kgStats)
  const kgSearchResults = useStore(s => s.kgSearchResults)
  const kgGraph = useStore(s => s.kgGraph)
  const kgNodeList = useStore(s => s.kgNodeList)
  const kgSearch = useStore(s => s.kgSearch)
  const kgExpandNode = useStore(s => s.kgExpandNode)
  const kgWalkFrom = useStore(s => s.kgWalkFrom)
  const kgLoadGraph = useStore(s => s.kgLoadGraph)
  const kgLoadStats = useStore(s => s.kgLoadStats)
  const kgClearGraph = useStore(s => s.kgClearGraph)
  const kgListNodes = useStore(s => s.kgListNodes)
  const kgNodeDelete = useStore(s => s.kgNodeDelete)
  const kgEdgeDelete = useStore(s => s.kgEdgeDelete)
  const showConfirm = useStore(s => s.showConfirm)
  const currentSessionId = useStore(s => s.currentSessionId)

  const [query, setQuery] = useState('')
  const [scopeFilter, setScopeFilter] = useState<'all' | 'global' | 'session'>('all')
  const [tooltip, setTooltip] = useState<{ node: GraphNode; x: number; y: number } | null>(null)
  const [activeTab, setActiveTab] = useState<'list' | 'graph'>('list')
  const [isFullscreen, setIsFullscreen] = useState(false)
  const [showLabels, setShowLabels] = useState(true)

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

  // Compute connected components ("galaxies") and assign each a distinct hue.
  // Nodes in the same galaxy share a base hue; USER is always warm gold.
  const galaxyColors = useMemo(() => {
    if (!kgGraph || kgGraph.nodes.length === 0) return new Map<string, { hue: number; galaxyIndex: number }>()

    // Build adjacency for BFS
    const adj = new Map<string, Set<string>>()
    for (const n of kgGraph.nodes) adj.set(n.name, new Set())
    for (const e of kgGraph.edges) {
      adj.get(e.source)?.add(e.target)
      adj.get(e.target)?.add(e.source)
    }

    // BFS over all nodes to find connected components
    const visited = new Set<string>()
    const component: string[][] = []
    for (const name of adj.keys()) {
      if (visited.has(name)) continue
      const queue = [name]
      const comp: string[] = []
      visited.add(name)
      while (queue.length > 0) {
        const cur = queue.shift()!
        comp.push(cur)
        for (const nb of adj.get(cur) ?? []) {
          if (!visited.has(nb)) { visited.add(nb); queue.push(nb) }
        }
      }
      component.push(comp)
    }

    // Assign hues: golden-ratio spacing for max colour separation
    const result = new Map<string, { hue: number; galaxyIndex: number }>()
    component.forEach((comp, ci) => {
      const hue = (ci * 137.508) % 360 // golden angle
      for (const name of comp) result.set(name, { hue, galaxyIndex: ci })
    })
    return result
  }, [kgGraph])

  // Convert KgGraph to graph-data format used by edge list and tooltip look-ups.
  const graphData = useMemo(() => {
    if (!kgGraph || kgGraph.nodes.length === 0) return { nodes: [] as GraphNode[], links: [] as GraphLink[] }

    const nodeMap = new Map<string, GraphNode>()
    for (const n of kgGraph.nodes) {
      const gc = galaxyColors.get(n.name)
      // USER is always gold; other nodes inherit their galaxy hue
      const color = n.name === 'USER'
        ? '#fbbf24'
        : gc
          ? `hsl(${gc.hue}, 68%, 62%)`
          : entityColor(n.entity_type) // fallback for isolated nodes without edges

      nodeMap.set(n.name, {
        id: n.name,
        node_id: n.node_id,
        name: n.name,
        entity_type: n.entity_type,
        description: n.description,
        confidence: n.confidence,
        scope: n.scope,
        color,
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
  }, [kgGraph])

  // Build ECharts option from graphData.
  const chartOption = useMemo(() => {
    const nodes = graphData.nodes.map(n => ({
      id: n.id,
      name: n.name,
      symbolSize: n.name === 'USER' ? 36 : 6 + n.confidence * 16,
      itemStyle: { color: n.color },
      label: { show: showLabels, color: '#e2e8f0', fontSize: 11 },
      // extra fields carried through for tooltip / click handler
      node_id: n.node_id,
      entity_type: n.entity_type,
      description: n.description,
      confidence: n.confidence,
      scope: n.scope,
    }))

    const links = graphData.links.map(l => ({
      source: l.source,
      target: l.target,
      value: l.relation_type,
      label: {
        show: showLabels,
        color: 'rgba(255,255,255,0.5)',
        fontSize: 9,
        formatter: (p: any) => translateRelation(p.data?.value ?? p.value ?? ''),
      },
    }))

    return {
      backgroundColor: 'transparent',
      tooltip: {
        trigger: 'item',
        formatter: (params: any) => {
          if (params.dataType === 'node') {
            const d = params.data
            let html = `<div style="max-width:260px;text-align:left">
              <div style="font-weight:600;font-size:13px;margin-bottom:4px;color:#e2e8f0">${d.name}</div>
              <div style="font-size:11px;color:#94a3b8;margin-bottom:4px">${d.entity_type}</div>`
            if (d.description) {
              html += `<div style="font-size:11px;color:#cbd5e1;margin-bottom:4px;line-height:1.5;word-break:break-word">${d.description}</div>`
            }
            html += `<div style="font-size:10px;color:#64748b">确信度 ${(d.confidence * 100).toFixed(0)}%</div>
            </div>`
            return html
          }
          if (params.dataType === 'edge') {
            return translateRelation(params.data?.value ?? params.value ?? '')
          }
          return ''
        },
      },
      series: [
        {
          type: 'graph',
          layout: 'force',
          roam: true,
          draggable: true,
          data: nodes,
          links,
          force: {
            repulsion: 3000,
            edgeLength: [400, 800],
            gravity: 0.1,
            friction: 0.6,
          },
          label: {
            show: showLabels,
            color: '#e2e8f0',
            fontSize: 11,
            position: 'bottom',
          },
          edgeLabel: {
            show: showLabels,
            color: 'rgba(255,255,255,0.5)',
            fontSize: 9,
            formatter: (p: any) => translateRelation(p.data?.value ?? p.value ?? ''),
          },
          lineStyle: {
            color: 'rgba(255,255,255,0.15)',
            curveness: 0.3,
          },
          emphasis: {
            focus: 'adjacency',
            lineStyle: { width: 3 },
          },
          animation: true,
          animationDuration: 800,
          animationDurationUpdate: 300,
          animationEasingUpdate: 'linearInOut',
        },
      ],
    }
  }, [graphData, showLabels])

  // ECharts event handlers.
  const onChartEvents = useMemo(
    () => ({
      click: (params: any) => {
        if (params.dataType === 'node') {
          const d = params.data
          setTooltip(prev => {
            if (prev?.node.id === d.id) return null
            const node: GraphNode = {
              id: d.id,
              node_id: d.node_id,
              name: d.name,
              entity_type: d.entity_type,
              description: d.description,
              confidence: d.confidence,
              scope: d.scope,
              color: entityColor(d.entity_type),
            }
            const ev: MouseEvent = params.event?.event ?? params.event
            return { node, x: ev?.clientX ?? 0, y: ev?.clientY ?? 0 }
          })
        }
      },
    }),
    [],
  )

  // Set up zr-level click handler to dismiss tooltip on background click.
  const handleChartReady = useCallback((instance: any) => {
    const zr = instance.getZr()
    zr.off('click')
    zr.on('click', (params: any) => {
      if (!params.target) {
        setTooltip(null)
      }
    })
  }, [])

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
      kgExpandNode(name)
    },
    [kgExpandNode],
  )

  const handleWalk = useCallback(
    (name: string) => {
      userClearedGraph.current = false
      setTooltip(null)
      kgWalkFrom(name)
    },
    [kgWalkFrom],
  )

  const handleDeleteNode = useCallback(
    async (name: string) => {
      const ok = await showConfirm('删除实体', `确定删除 "${name}" 及其所有关系？`, true)
      if (!ok) return
      kgNodeDelete(name)
    },
    [kgNodeDelete, showConfirm],
  )

  const handleClearGraph = useCallback(() => {
    userClearedGraph.current = true
    kgClearGraph()
  }, [kgClearGraph])

  const handleDeleteEdge = useCallback(
    async (source: string, target: string, relation: string) => {
      kgEdgeDelete(source, target, relation)
    },
    [kgEdgeDelete],
  )

  const hasData = graphData.nodes.length > 0
  // search results overlay on top of node list; clear when query is empty
  const displayNodes = query && kgSearchResults.length > 0 ? kgSearchResults : kgNodeList

  return (
    <div className={styles.panel}>
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
          <span>
            实体 <span className={styles.statValue}>{kgStats.node_count}</span>
          </span>
          <span>
            关系 <span className={styles.statValue}>{kgStats.edge_count}</span>
          </span>
        </div>
      )}

      {/* Tabs */}
      <div className={styles.tabs}>
        <button
          className={`${styles.tab} ${activeTab === 'list' ? styles.tabActive : ''}`}
          onClick={() => setActiveTab('list')}
        >
          实体列表
        </button>
        <button
          className={`${styles.tab} ${activeTab === 'graph' ? styles.tabActive : ''}`}
          onClick={() => setActiveTab('graph')}
        >
          图谱星图
        </button>
        {activeTab === 'graph' && (
          <>
            <button
              className={styles.labelToggleBtn}
              onClick={() => setShowLabels(v => !v)}
              title={showLabels ? '隐藏标签' : '显示标签'}
            >
              {showLabels ? 'Aa' : 'Aa'}
            </button>
            <button
              className={styles.fullscreenBtn}
              onClick={() => setIsFullscreen(!isFullscreen)}
              title={isFullscreen ? '退出全屏' : '全屏查看'}
            >
              {isFullscreen ? '⤓' : '⤢'}
            </button>
            {kgGraph && (
              <button className={styles.clearBtn} onClick={handleClearGraph}>
                清除图谱
              </button>
            )}
          </>
        )}
      </div>

      {activeTab === 'list' && (
        <div className={styles.nodeList}>
          {displayNodes.length === 0 ? (
            <div className={styles.canvasEmpty}>
              暂无实体。知识图谱会在与 AI 对话过程中自动积累。
            </div>
          ) : (
            displayNodes.map(n => (
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
                      {n.entity_type}
                    </span>
                    {n.scope && n.scope !== 'global' && (
                      <span className={styles.scopeBadge}>{n.scope.slice(0, 6)}</span>
                    )}
                    <span className={styles.nodeItemConf} title="AI 对该实体信息的把握程度">
                      确信 {(n.confidence * 100).toFixed(0)}%
                    </span>
                  </div>
                  <div className={styles.nodeItemActions}>
                    <button className={styles.actionBtn} onClick={() => handleExpand(n.name)}>
                      关联
                    </button>
                    <button className={styles.actionBtn} onClick={() => handleWalk(n.name)}>
                      关系网
                    </button>
                    <span className={styles.actionDivider} />
                    <button className={styles.actionBtnDanger} onClick={() => handleDeleteNode(n.name)}>
                      删除
                    </button>
                  </div>
                </div>
                {n.description && <div className={styles.nodeItemDesc}>{n.description}</div>}
              </div>
            ))
          )}
        </div>
      )}

      {activeTab === 'graph' && (
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
              <ReactEChartsCore
                echarts={echarts}
                option={chartOption}
                style={{ width: '100%', height: '100%' }}
                onChartReady={handleChartReady}
                onEvents={onChartEvents}
                notMerge={true}
                lazyUpdate={true}
              />
            )}
          </div>

          {/* Edge list */}
          {hasData && (
            <div className={styles.edgeList}>
              <div className={styles.edgeListTitle}>关系列表 ({graphData.links.length})</div>
              {graphData.links.map((l, i) => {
                const srcName = typeof l.source === 'object' ? (l.source as any).name || (l.source as any).id : l.source
                const tgtName = typeof l.target === 'object' ? (l.target as any).name || (l.target as any).id : l.target
                return (
                  <div key={i} className={styles.edgeItem}>
                    <span className={styles.edgeNode}>{srcName}</span>
                    <span className={styles.edgeRel}>{translateRelation(l.relation_type) || '相关'}</span>
                    <span className={styles.edgeNode}>{tgtName}</span>
                    <button
                      className={styles.edgeDelBtn}
                      onClick={() =>
                        handleDeleteEdge(srcName as string, tgtName as string, l.relation_type)
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
          const types = [...new Set(graphData.nodes.map(n => n.entity_type))]
          return types.map(type => (
            <div key={type} className={styles.legendItem}>
              <span className={styles.legendDot} style={{ background: entityColor(type) }} />
              {type}
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
          <div className={styles.tooltipType}>{tooltip.node.entity_type}</div>
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
          <ReactEChartsCore
            echarts={echarts}
            option={chartOption}
            style={{ width: '100%', height: '100%' }}
            onChartReady={handleChartReady}
            onEvents={onChartEvents}
            notMerge={true}
            lazyUpdate={true}
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
