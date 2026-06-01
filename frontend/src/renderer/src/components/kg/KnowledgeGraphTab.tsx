import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import ForceGraph2D from 'react-force-graph-2d'
import { forceCollide, forceManyBody } from 'd3-force'
import { useStore } from '../../stores'
import Select from '../shared/Select'
import type { KgNode } from '../../types/bindings'
import styles from './KnowledgeGraphPanel.module.css'

// Seeded PRNG for stable star field layout
function mulberry32(seed: number) {
  return () => {
    seed |= 0; seed = seed + 0x6D2B79F5 | 0
    let t = Math.imul(seed ^ seed >>> 15, 1 | seed)
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t
    return ((t ^ t >>> 14) >>> 0) / 4294967296
  }
}

const ENTITY_COLORS: Record<string, string> = {
  Person: '#22d3ee',
  Technology: '#a78bfa',
  Topic: '#fb923c',
  Project: '#34d399',
  Concept: '#f472b6',
  Tool: '#818cf8',
  Organization: '#fbbf24',
}
const DEFAULT_COLOR = '#9ca3af'

const RELATION_LABELS: Record<string, string> = {
  uses: '使用', works_on: '参与', knows: '了解',
  interested_in: '感兴趣', dislikes: '不喜欢', depends_on: '依赖',
  part_of: '属于', created_by: '创建者', related_to: '相关',
}

function translateRelation(rel: string): string {
  return RELATION_LABELS[rel] ?? rel.replace(/_/g, ' ')
}

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
  scope: string
  color: string
  val: number
}

interface GraphLink {
  source: string
  target: string
  relation_type: string
  confidence: number
  /** Index of this link among parallel links between the same pair (0-based) */
  linkIndex: number
  /** Total number of parallel links between the same pair */
  linkCount: number
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
  const fgRef = useRef<any>(null)
  const fgFullscreenRef = useRef<any>(null)
  const wrapRef = useRef<HTMLDivElement>(null)
  const fullscreenWrapRef = useRef<HTMLDivElement>(null)
  const starCanvasRef = useRef<HTMLCanvasElement>(null)
  const fullscreenStarCanvasRef = useRef<HTMLCanvasElement>(null)
  const [dimensions, setDimensions] = useState({ w: 600, h: 400 })
  const [fullscreenDimensions, setFullscreenDimensions] = useState({ w: 1200, h: 800 })
  /** Suppresses auto-populate after user intentionally clears the graph */
  const userClearedGraph = useRef(false)
  const initialFitDone = useRef(false)

  // Read theme colors for canvas rendering
  const [themeColors, setThemeColors] = useState({
    bg: '#0B0F14',
    text: 'rgba(255,255,255,0.88)',
    textSecondary: 'rgba(255,255,255,0.60)',
    textMuted: 'rgba(255,255,255,0.30)',
    border: 'rgba(255,255,255,0.06)',
  })

  useEffect(() => {
    const updateThemeColors = () => {
      const style = getComputedStyle(document.documentElement)
      setThemeColors({
        bg: style.getPropertyValue('--bg').trim() || '#0B0F14',
        text: style.getPropertyValue('--text').trim() || 'rgba(255,255,255,0.88)',
        textSecondary: style.getPropertyValue('--text-secondary').trim() || 'rgba(255,255,255,0.60)',
        textMuted: style.getPropertyValue('--text-muted').trim() || 'rgba(255,255,255,0.30)',
        border: style.getPropertyValue('--border').trim() || 'rgba(255,255,255,0.06)',
      })
    }
    updateThemeColors()
    // Re-read colors when theme changes (observe class changes on html element)
    const observer = new MutationObserver(updateThemeColors)
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme'] })
    return () => observer.disconnect()
  }, [])

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
      const dataAvailable = !!kgGraph && kgGraph.nodes.length > 0
      console.log('[KnowledgeGraphTab] Graph data changed:', {
        hasKgGraph: !!kgGraph,
        nodeCount: kgGraph?.nodes.length || 0,
        edgeCount: kgGraph?.edges.length || 0,
        dimensions,
        hasData: dataAvailable,
        nodes: kgGraph?.nodes.map(n => n.name),
        edges: kgGraph?.edges.map(e => `${e.source} -> ${e.target}`),
      })
    }
  }, [activeTab, kgGraph, dimensions])

  useEffect(() => {
    const el = wrapRef.current
    if (!el || activeTab !== 'graph') return

    const measure = () => {
      const rect = el.getBoundingClientRect()
      const w = Math.ceil(rect.width)
      const h = Math.max(Math.ceil(rect.height), 300)
      // Only update if dimensions are valid
      if (w > 0 && h > 0) {
        setDimensions({ w, h })
      }
    }

    // Measure immediately
    measure()

    // Then observe for changes
    const ro = new ResizeObserver(() => measure())
    ro.observe(el)

    // Also measure after a short delay to ensure layout is complete
    const timer = setTimeout(measure, 100)

    return () => {
      ro.disconnect()
      clearTimeout(timer)
    }
  }, [activeTab])

  // Fullscreen dimension measurement
  useEffect(() => {
    if (!isFullscreen) return
    const measure = () => {
      setFullscreenDimensions({
        w: window.innerWidth,
        h: window.innerHeight,
      })
    }
    window.addEventListener('resize', measure)
    measure()
    return () => window.removeEventListener('resize', measure)
  }, [isFullscreen])

  // Shared star field rendering function
  const renderStarField = (
    canvas: HTMLCanvasElement,
    w: number,
    h: number,
  ): (() => void) => {
    const dpr = window.devicePixelRatio || 1
    canvas.width = w * dpr
    canvas.height = h * dpr
    canvas.style.width = w + 'px'
    canvas.style.height = h + 'px'
    const ctx = canvas.getContext('2d')
    if (!ctx) return () => {}
    ctx.scale(dpr, dpr)

    // Generate star data once
    const rng = mulberry32(42)
    const starCount = Math.floor((w * h) / 400)
    const stars = []
    for (let i = 0; i < starCount; i++) {
      stars.push({
        x: rng() * w,
        y: rng() * h,
        r: rng() * 1.2 + 0.2,
        baseBrightness: rng() * 0.5 + 0.1,
        phase: rng() * Math.PI * 2,
        speed: rng() * 0.002 + 0.001,
      })
    }

    // Generate bright stars with flares
    const brightStars = []
    for (let i = 0; i < Math.floor(starCount * 0.02); i++) {
      brightStars.push({
        x: rng() * w,
        y: rng() * h,
        r: rng() * 0.8 + 0.5,
        baseBrightness: rng() * 0.3 + 0.5,
        phase: rng() * Math.PI * 2,
        speed: rng() * 0.003 + 0.002,
      })
    }

    // Generate nebula data
    const nebulaColors = [
      'rgba(34, 211, 238, 0.012)',
      'rgba(167, 139, 250, 0.010)',
      'rgba(244, 114, 182, 0.008)',
      'rgba(52, 211, 153, 0.008)',
    ]
    const nebulae = []
    for (let i = 0; i < 6; i++) {
      nebulae.push({
        x: rng() * w,
        y: rng() * h,
        r: rng() * Math.min(w, h) * 0.4 + 80,
        color: nebulaColors[i % nebulaColors.length],
        phase: rng() * Math.PI * 2,
        speed: rng() * 0.0005 + 0.0002,
      })
    }

    let animationId: number
    const draw = () => {
      const time = Date.now()

      // Deep space background
      const bgGrad = ctx.createRadialGradient(w / 2, h / 2, 0, w / 2, h / 2, Math.max(w, h) * 0.7)
      bgGrad.addColorStop(0, '#0d1117')
      bgGrad.addColorStop(0.5, '#080b10')
      bgGrad.addColorStop(1, '#050709')
      ctx.fillStyle = bgGrad
      ctx.fillRect(0, 0, w, h)

      // Animated nebulae with subtle drift
      for (const neb of nebulae) {
        const drift = Math.sin(time * neb.speed + neb.phase) * 20
        const grad = ctx.createRadialGradient(
          neb.x + drift, neb.y, 0,
          neb.x + drift, neb.y, neb.r
        )
        grad.addColorStop(0, neb.color)
        grad.addColorStop(1, 'transparent')
        ctx.fillStyle = grad
        ctx.fillRect(0, 0, w, h)
      }

      // Twinkling stars
      for (const star of stars) {
        const twinkle = Math.sin(time * star.speed + star.phase) * 0.3 + 0.7
        const brightness = star.baseBrightness * twinkle
        ctx.beginPath()
        ctx.arc(star.x, star.y, star.r, 0, Math.PI * 2)
        ctx.fillStyle = `rgba(255,255,255,${brightness})`
        ctx.fill()
      }

      // Bright stars with animated flares
      for (const star of brightStars) {
        const twinkle = Math.sin(time * star.speed + star.phase) * 0.4 + 0.8
        const brightness = star.baseBrightness * twinkle

        ctx.beginPath()
        ctx.arc(star.x, star.y, star.r, 0, Math.PI * 2)
        ctx.fillStyle = `rgba(255,255,255,${brightness})`
        ctx.fill()

        // Animated cross flare
        const flareLen = star.r * 6 * twinkle
        ctx.strokeStyle = `rgba(255,255,255,${brightness * 0.3})`
        ctx.lineWidth = 0.5
        ctx.beginPath()
        ctx.moveTo(star.x - flareLen, star.y)
        ctx.lineTo(star.x + flareLen, star.y)
        ctx.stroke()
        ctx.beginPath()
        ctx.moveTo(star.x, star.y - flareLen)
        ctx.lineTo(star.x, star.y + flareLen)
        ctx.stroke()
      }

      animationId = requestAnimationFrame(draw)
    }

    draw()
    return () => {
      if (animationId) cancelAnimationFrame(animationId)
    }
  }

  // Stop force graph animation on unmount to prevent D3 zoom
  // document-level event listener leaks blocking clicks on the nav
  const fgInstanceRef = useRef<any>(null)
  const fgFullscreenInstanceRef = useRef<any>(null)
  useEffect(() => {
    fgInstanceRef.current = fgRef.current
    fgFullscreenInstanceRef.current = fgFullscreenRef.current
  })
  useEffect(() => {
    return () => {
      fgInstanceRef.current?.stopAnimation?.()
      fgFullscreenInstanceRef.current?.stopAnimation?.()
    }
  }, [])

  // Animated star field background for normal view
  useEffect(() => {
    const canvas = starCanvasRef.current
    if (!canvas || activeTab !== 'graph') return
    const { w, h } = dimensions
    if (w < 10 || h < 10) return
    return renderStarField(canvas, w, h)
  }, [dimensions, activeTab])

  // Animated star field background for fullscreen view
  useEffect(() => {
    const canvas = fullscreenStarCanvasRef.current
    if (!canvas || !isFullscreen) return
    const { w, h } = fullscreenDimensions
    if (w < 10 || h < 10) return
    return renderStarField(canvas, w, h)
  }, [fullscreenDimensions, isFullscreen])

  // Convert KgGraph to react-force-graph format.
  // Memoized so tooltip toggle and other renders don't re-simulate the graph.
  const graphData = useMemo(() => {
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
        scope: n.scope,
        color: entityColor(n.entity_type),
        val: n.name === 'USER' ? 18 : 3 + n.confidence * 8,
      })
    }

    const links: GraphLink[] = []
    const pairCount = new Map<string, number>()
    const pairIndex = new Map<string, number>()
    for (const e of kgGraph.edges) {
      if (nodeMap.has(e.source) && nodeMap.has(e.target)) {
        const key = [e.source, e.target].sort().join('\0')
        pairCount.set(key, (pairCount.get(key) ?? 0) + 1)
      }
    }
    for (const e of kgGraph.edges) {
      if (nodeMap.has(e.source) && nodeMap.has(e.target)) {
        const key = [e.source, e.target].sort().join('\0')
        const count = pairCount.get(key) ?? 1
        const idx = pairIndex.get(key) ?? 0
        pairIndex.set(key, idx + 1)
        links.push({
          source: e.source,
          target: e.target,
          relation_type: e.relation_type,
          confidence: e.confidence,
          linkIndex: idx,
          linkCount: count,
        })
      }
    }

    return { nodes: [...nodeMap.values()], links }
  }, [kgGraph])

  const handleSearch = useCallback(() => {
    if (!query.trim()) return
    kgSearch(query.trim())
  }, [query, kgSearch])

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleSearch()
  }, [handleSearch])

  const handleExpand = useCallback((name: string) => {
    userClearedGraph.current = false
    setTooltip(null)
    kgExpandNode(name)
  }, [kgExpandNode])

  const handleWalk = useCallback((name: string) => {
    userClearedGraph.current = false
    setTooltip(null)
    kgWalkFrom(name)
  }, [kgWalkFrom])

  // Canvas paint for nodes — cosmic star style
  const paintNode = useCallback((node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
    const n = node as GraphNode

    // Guard: skip if coordinates not yet computed by force simulation
    if (!Number.isFinite(n.x) || !Number.isFinite(n.y)) return

    const r = Math.max(2, n.val)
    const fontSize = Math.max(9, 11 / globalScale)
    const label = n.name.length > 10 ? n.name.slice(0, 10) + '..' : n.name

    // Outer atmosphere (wide, very faint)
    const atmoGrad = ctx.createRadialGradient(n.x!, n.y!, r, n.x!, n.y!, r * 8)
    atmoGrad.addColorStop(0, n.color + '20')
    atmoGrad.addColorStop(0.3, n.color + '0a')
    atmoGrad.addColorStop(1, n.color + '00')
    ctx.beginPath()
    ctx.arc(n.x!, n.y!, r * 8, 0, Math.PI * 2)
    ctx.fillStyle = atmoGrad
    ctx.fill()

    // Inner glow (bright halo)
    const glowGrad = ctx.createRadialGradient(n.x!, n.y!, 0, n.x!, n.y!, r * 3)
    glowGrad.addColorStop(0, n.color + '88')
    glowGrad.addColorStop(0.5, n.color + '30')
    glowGrad.addColorStop(1, n.color + '00')
    ctx.beginPath()
    ctx.arc(n.x!, n.y!, r * 3, 0, Math.PI * 2)
    ctx.fillStyle = glowGrad
    ctx.fill()

    // Core (bright star center)
    ctx.beginPath()
    ctx.arc(n.x!, n.y!, r, 0, Math.PI * 2)
    ctx.fillStyle = n.color
    ctx.fill()

    // Hot white center (like a real star)
    ctx.beginPath()
    ctx.arc(n.x!, n.y!, r * 0.5, 0, Math.PI * 2)
    ctx.fillStyle = 'rgba(255,255,255,0.9)'
    ctx.fill()

    // Subtle cross-flare for brighter nodes
    if (n.val > 5) {
      const flareLen = r * 4
      const flareAlpha = Math.min(0.3, (n.val - 5) * 0.05)
      ctx.strokeStyle = `${n.color}${Math.round(flareAlpha * 255).toString(16).padStart(2, '0')}`
      ctx.lineWidth = 0.5 / globalScale
      ctx.beginPath(); ctx.moveTo(n.x! - flareLen, n.y!); ctx.lineTo(n.x! + flareLen, n.y!); ctx.stroke()
      ctx.beginPath(); ctx.moveTo(n.x!, n.y! - flareLen); ctx.lineTo(n.x!, n.y! + flareLen); ctx.stroke()
    }

    // Label below node (togglable)
    if (showLabels) {
      ctx.font = `${fontSize}px -apple-system, "Microsoft YaHei", sans-serif`
      ctx.fillStyle = themeColors.text
      ctx.textAlign = 'center'
      ctx.fillText(label, n.x!, n.y! + r + fontSize + 2)
    }
  }, [themeColors, showLabels])

  // Canvas paint for links — constellation beam style
  const paintLink = useCallback((link: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
    const src = link.source
    const tgt = link.target
    if (!Number.isFinite(src.x) || !Number.isFinite(src.y) || !Number.isFinite(tgt.x) || !Number.isFinite(tgt.y)) return

    const dx = tgt.x - src.x
    const dy = tgt.y - src.y
    const len = Math.sqrt(dx * dx + dy * dy) || 1
    // Perpendicular unit vector for offsetting parallel links
    const nx = -dy / len
    const ny = dx / len
    const linkIndex = (link as GraphLink).linkIndex ?? 0
    const linkCount = (link as GraphLink).linkCount ?? 1
    // Larger offset: 28px base, scales inversely with zoom so labels stay separated when zoomed out
    const offsetAmount = 28 / globalScale
    const offset = (linkIndex - (linkCount - 1) / 2) * offsetAmount
    const mx = (src.x + tgt.x) / 2 + nx * offset
    const my = (src.y + tgt.y) / 2 + ny * offset

    // Glow beam (wide, faint)
    ctx.beginPath()
    ctx.moveTo(src.x, src.y)
    ctx.lineTo(tgt.x, tgt.y)
    ctx.strokeStyle = 'rgba(255,255,255,0.04)'
    ctx.lineWidth = 4 / globalScale
    ctx.stroke()

    // Core line
    ctx.beginPath()
    ctx.moveTo(src.x, src.y)
    ctx.lineTo(tgt.x, tgt.y)
    ctx.strokeStyle = 'rgba(255,255,255,0.15)'
    ctx.lineWidth = 0.8 / globalScale
    ctx.stroke()

    // Edge label at midpoint (togglable)
    if (showLabels && link.relation_type) {
      const fontSize = Math.max(7, 9 / globalScale)
      const text = translateRelation(link.relation_type)
      ctx.font = `${fontSize}px -apple-system, "Microsoft YaHei", sans-serif`
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      const tw = ctx.measureText(text).width
      const pad = 3 / globalScale
      ctx.fillStyle = 'rgba(5,7,9,0.75)'
      ctx.fillRect(mx - tw / 2 - pad, my - fontSize / 2 - pad, tw + pad * 2, fontSize + pad * 2)
      ctx.fillStyle = themeColors.textSecondary
      ctx.fillText(text, mx, my)
    }
  }, [themeColors, showLabels])

  const handleNodeClick = useCallback((node: any) => {
    const n = node as GraphNode
    setTooltip(prev => {
      if (prev?.node.id === n.id) return null
      // Use fullscreen graph ref if available, otherwise normal ref
      const fg = fgFullscreenRef.current || fgRef.current
      if (fg && Number.isFinite(n.x) && Number.isFinite(n.y)) {
        const screen = fg.graph2ScreenCoords(n.x, n.y)
        return { node: n, x: screen.x, y: screen.y }
      }
      return { node: n, x: 0, y: 0 }
    })
  }, [])

  const handleDeleteNode = useCallback(async (name: string) => {
    const ok = await showConfirm('删除实体', `确定删除 "${name}" 及其所有关系？`, true)
    if (!ok) return
    kgNodeDelete(name)
  }, [kgNodeDelete, showConfirm])

  const handleClearGraph = useCallback(() => {
    userClearedGraph.current = true
    kgClearGraph()
  }, [kgClearGraph])

  const handleDeleteEdge = useCallback(async (source: string, target: string, relation: string) => {
    kgEdgeDelete(source, target, relation)
  }, [kgEdgeDelete])

  const hasData = graphData.nodes.length > 0
  // Reset zoom-to-fit when graph data changes (expand/walk/delete)
  useEffect(() => { initialFitDone.current = false }, [graphData])
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
          onChange={(v) => setScopeFilter(v as 'all' | 'global' | 'session')}
          variant="form"
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
        >图谱星图</button>
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
              <button className={styles.clearBtn} onClick={handleClearGraph}>清除图谱</button>
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
                      style={{ color: entityColor(n.entity_type), background: entityColor(n.entity_type) + '18' }}
                    >{n.entity_type}</span>
                    {n.scope && n.scope !== 'global' && (
                      <span className={styles.scopeBadge}>{n.scope.slice(0, 6)}</span>
                    )}
                    <span className={styles.nodeItemConf} title="AI 对该实体信息的把握程度">确信 {(n.confidence * 100).toFixed(0)}%</span>
                  </div>
                  <div className={styles.nodeItemActions}>
                    <button className={styles.actionBtn} onClick={() => handleExpand(n.name)}>关联</button>
                    <button className={styles.actionBtn} onClick={() => handleWalk(n.name)}>关系网</button>
                    <span className={styles.actionDivider} />
                    <button className={styles.actionBtnDanger} onClick={() => handleDeleteNode(n.name)}>删除</button>
                  </div>
                </div>
                {n.description && (
                  <div className={styles.nodeItemDesc}>{n.description}</div>
                )}
              </div>
            ))
          )}
        </div>
      )}

      {activeTab === 'graph' && (
        <>
          <div className={styles.canvasWrap} ref={wrapRef}>
            <canvas ref={starCanvasRef} className={styles.starField} />
            {!hasData ? (
              <div className={styles.canvasEmpty}>
                <span>{kgNodeList.length > 0 ? '正在加载图谱...' : '星图暂无数据'}</span>
                {kgNodeList.length === 0 && (
                  <span className={styles.canvasEmptyHint}>
                    在与 AI 对话过程中，知识图谱会自动积累实体与关系，构建属于你的认知网络
                  </span>
                )}
              </div>
            ) : dimensions.w < 10 || dimensions.h < 10 ? (
              <div className={styles.canvasEmpty}>正在初始化...</div>
            ) : (
              <ForceGraph2D
                ref={fgRef}
                graphData={graphData}
                width={dimensions.w}
                height={dimensions.h}
                backgroundColor="transparent"
                nodeCanvasObject={paintNode}
                nodePointerAreaPaint={(node: any, color: string, ctx: CanvasRenderingContext2D) => {
                  const n = node as GraphNode
                  if (!Number.isFinite(n.x) || !Number.isFinite(n.y)) return
                  // Match visible glow extent (inner glow is r*3)
                  const r = Math.max(2, n.val) * 3
                  ctx.beginPath()
                  ctx.arc(n.x!, n.y!, r, 0, Math.PI * 2)
                  ctx.fillStyle = color
                  ctx.fill()
                }}
                linkCanvasObject={paintLink}
                linkDirectionalArrowLength={0}
                linkDistance={(link: any) => {
                  const s = link.source as GraphNode
                  const t = link.target as GraphNode
                  if (s.scope && t.scope && s.scope === t.scope && s.scope !== 'global') {
                    return 1200
                  }
                  return 1800
                }}
                linkStrength={(link: any) => {
                  const s = link.source as GraphNode
                  const t = link.target as GraphNode
                  if (s.scope && t.scope && s.scope === t.scope && s.scope !== 'global') {
                    return 0.8
                  }
                  return 0.4
                }}
                enableZoomInteraction
                enablePanInteraction
                minZoom={0.2}
                maxZoom={5}
                cooldownTicks={400}
                d3AlphaDecay={0.003}
                d3VelocityDecay={0.4}
                d3Force={(engine: any) => {
                  // Kill center force — it pulls everything together
                  engine.force('center', null)
                  // Anti-collision
                  engine.force('collide', forceCollide((n: GraphNode) => Math.max(2, n.val) * 20))
                  // Strong repulsion between all nodes (galaxy-like spread)
                  engine.force('charge', forceManyBody().strength(-1200).distanceMin(80).distanceMax(4000))
                }}
                onEngineStop={() => {
                  if (!initialFitDone.current) {
                    initialFitDone.current = true
                    fgRef.current?.zoomToFit(400, 120)
                  }
                }}
                onNodeClick={handleNodeClick}
                onBackgroundClick={() => setTooltip(null)}
              />
            )}

            {tooltip && (
              <div className={styles.tooltip} style={{ left: tooltip.x + 12, top: tooltip.y - 12 }}>
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
                    onClick={() => handleDeleteEdge(srcName as string, tgtName as string, l.relation_type)}
                    title="删除关系"
                  >x</button>
                </div>
                )
              })}
            </div>
          )}
        </>
      )}

      <div className={styles.legend}>
        {Object.entries(ENTITY_COLORS).map(([type, color]) => (
          <div key={type} className={styles.legendItem}>
            <span className={styles.legendDot} style={{ background: color }} />
            {type}
          </div>
        ))}
      </div>

      {/* Fullscreen portal */}
      {isFullscreen && activeTab === 'graph' && hasData && (
        <div className={styles.fullscreenOverlay} ref={fullscreenWrapRef}>
          <canvas ref={fullscreenStarCanvasRef} className={styles.starField} />
          <ForceGraph2D
            ref={fgFullscreenRef}
            graphData={graphData}
            width={fullscreenDimensions.w}
            height={fullscreenDimensions.h}
            backgroundColor="transparent"
            nodeCanvasObject={paintNode}
            nodePointerAreaPaint={(node: any, color: string, ctx: CanvasRenderingContext2D) => {
              const n = node as GraphNode
              if (!Number.isFinite(n.x) || !Number.isFinite(n.y)) return
              const r = Math.max(2, n.val) * 4
              ctx.beginPath()
              ctx.arc(n.x!, n.y!, r, 0, Math.PI * 2)
              ctx.fillStyle = color
              ctx.fill()
            }}
            linkCanvasObject={paintLink}
            linkDirectionalArrowLength={0}
            linkDistance={(link: any) => {
              const s = link.source as GraphNode
              const t = link.target as GraphNode
              if (s.scope && t.scope && s.scope === t.scope && s.scope !== 'global') {
                return 120
              }
              return 200
            }}
            linkStrength={(link: any) => {
              const s = link.source as GraphNode
              const t = link.target as GraphNode
              if (s.scope && t.scope && s.scope === t.scope && s.scope !== 'global') {
                return 1.5
              }
              return 1.0
            }}
            enableZoomInteraction
            enablePanInteraction
            minZoom={0.1}
            maxZoom={8}
            cooldownTicks={200}
            d3AlphaDecay={0.01}
            d3VelocityDecay={0.2}
            onEngineStop={() => {
              if (!initialFitDone.current) {
                initialFitDone.current = true
                fgFullscreenRef.current?.zoomToFit(400, 50)
              }
            }}
            onNodeClick={handleNodeClick}
            onBackgroundClick={() => setTooltip(null)}
          />
          {tooltip && (
            <div className={styles.tooltip} style={{ left: tooltip.x + 12, top: tooltip.y - 12 }}>
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
          <button
            className={styles.fullscreenLabelBtn}
            onClick={() => setShowLabels(v => !v)}
            title={showLabels ? '隐藏标签' : '显示标签'}
          >
            {showLabels ? 'Aa' : 'Aa'}
          </button>
          <button
            className={styles.fullscreenCloseBtn}
            onClick={() => { setTooltip(null); setIsFullscreen(false) }}
            title="退出全屏"
          >
            ✕
          </button>
        </div>
      )}
    </div>
  )
}
