import { useRef, useEffect, useCallback } from 'react'
import { useLocale } from '../../i18n'

interface GraphNode {
  id: string
  name: string
  entity_type: string
  description: string
  confidence: number
  scope: string
  color: string
}

interface GraphEdge {
  source: string
  target: string
  relation_type: string
  confidence: number
}

interface Position {
  x: number
  y: number
  vx: number
  vy: number
  pinned: boolean
}

interface GalaxyGraphProps {
  nodes: { name: string; entity_type: string; description: string; confidence: number; scope: string }[]
  edges: { source: string; target: string; relation_type: string; confidence: number }[]
  showLabels: boolean
  onNodeClick: (node: GraphNode, clientX: number, clientY: number) => void
  onBackgroundClick: () => void
}

function entityHue(type: string): number {
  let h = 0
  for (let i = 0; i < type.length; i++) h = ((h << 5) - h + type.charCodeAt(i)) | 0
  return Math.abs(h) % 360
}

function entityColor(type: string): string {
  return `hsl(${entityHue(type)}, 70%, 62%)`
}

function pseudoRandom(seed: number) {
  return () => {
    seed = (seed * 16807 + 0) % 2147483647
    return (seed - 1) / 2147483646
  }
}

function hashString(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) h = ((h << 5) - h + s.charCodeAt(i)) | 0
  return Math.abs(h)
}

const PHYSICS = {
  repulsion: 8000,
  springStrength: 0.004,
  springRest: 250,
  damping: 0.88,
  maxForce: 8,
  stopEnergy: 0.05,
  gravity: 0.00035,
  gridCellSize: 250,
}

// ── Spatial Hash Grid ──────────────────────────────────────────────

interface GridCell {
  x: number
  y: number
  indices: number[]
}

function buildSpatialHash(
  nodeList: { name: string }[],
  positions: Map<string, Position>,
  cellSize: number,
): Map<string, GridCell> {
  const grid = new Map<string, GridCell>()
  for (let i = 0; i < nodeList.length; i++) {
    const p = positions.get(nodeList[i].name)
    if (!p || p.pinned) continue
    const cx = Math.floor(p.x / cellSize)
    const cy = Math.floor(p.y / cellSize)
    const key = `${cx},${cy}`
    let cell = grid.get(key)
    if (!cell) {
      cell = { x: cx, y: cy, indices: [] }
      grid.set(key, cell)
    }
    cell.indices.push(i)
  }
  return grid
}

const NEIGHBOR_OFFSETS = [
  [-1, -1], [0, -1], [1, -1],
  [-1,  0],          [1,  0],
  [-1,  1], [0,  1], [1,  1],
]

function getNeighborIndices(
  grid: Map<string, GridCell>,
  cx: number,
  cy: number,
): number[] {
  const result: number[] = []
  const selfKey = `${cx},${cy}`
  const self = grid.get(selfKey)
  if (self) result.push(...self.indices)

  for (const [dx, dy] of NEIGHBOR_OFFSETS) {
    const key = `${cx + dx},${cy + dy}`
    const cell = grid.get(key)
    if (cell) result.push(...cell.indices)
  }
  return result
}

// ── Node Sprite Cache ──────────────────────────────────────────────

function createNodeSprite(hue: number, sat: number, lit: number, r: number, isUser: boolean): HTMLCanvasElement {
  const size = Math.ceil(r * 5)
  const off = document.createElement('canvas')
  off.width = size
  off.height = size
  const ctx = off.getContext('2d')!
  const cx = size / 2
  const cy = size / 2

  const bloomR = r * (isUser ? 4.0 : 3.0)
  const bloom = ctx.createRadialGradient(cx, cy, r * 0.2, cx, cy, bloomR)
  bloom.addColorStop(0,    `hsla(${hue}, ${sat}%, ${Math.min(lit + 18, 92)}%, 0.55)`)
  bloom.addColorStop(0.35, `hsla(${hue}, ${sat}%, ${lit}%, 0.18)`)
  bloom.addColorStop(1,    'rgba(0,0,0,0)')
  ctx.beginPath()
  ctx.arc(cx, cy, bloomR, 0, Math.PI * 2)
  ctx.fillStyle = bloom
  ctx.fill()

  ctx.beginPath()
  ctx.arc(cx, cy, r, 0, Math.PI * 2)
  ctx.fillStyle = `hsl(${hue}, ${sat}%, ${lit}%)`
  ctx.fill()

  const hotR = r * 0.5
  const hot = ctx.createRadialGradient(cx, cy, 0, cx, cy, hotR)
  hot.addColorStop(0, 'rgba(255,255,255,0.92)')
  hot.addColorStop(0.5, 'rgba(255,255,255,0.35)')
  hot.addColorStop(1, 'rgba(255,255,255,0)')
  ctx.beginPath()
  ctx.arc(cx, cy, hotR, 0, Math.PI * 2)
  ctx.fillStyle = hot
  ctx.fill()

  return off
}

// ── Idle frame rates ───────────────────────────────────────────────
const IDLE_FPS = 4          // fps when graph is stable and user is idle
const IDLE_INTERVAL = 1000 / IDLE_FPS

export default function GalaxyGraph({ nodes, edges, showLabels, onNodeClick, onBackgroundClick }: GalaxyGraphProps) {
  const { t } = useLocale()
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const positionsRef = useRef<Map<string, Position>>(new Map())
  const transformRef = useRef({ x: 0, y: 0, scale: 1 })
  const dragRef = useRef<{
    type: 'pan' | 'node'
    nodeName?: string
    sx: number; sy: number
    px: number; py: number
    tx: number; ty: number
    moved: boolean
    neighborPins?: Map<string, { px: number; py: number }>
  } | null>(null)
  const animRef = useRef<number>(0)
  const nodesRef = useRef(nodes)
  const edgesRef = useRef(edges)
  nodesRef.current = nodes
  edgesRef.current = edges

  const translateRel = useCallback((rel: string): string => {
    return t(`kg.relation.${rel}`) ?? rel.replace(/_/g, ' ')
  }, [t])

  // ── Draw scheduling ──────────────────────────────────────────────
  const needsDrawRef = useRef(true)
  const userActiveRef = useRef(false)     // true during drag/zoom
  const lastDrawTimeRef = useRef(0)

  const requestDraw = useCallback(() => {
    needsDrawRef.current = true
  }, [])

  // ── Sprite cache ──────────────────────────────────────────────────
  const spriteCacheRef = useRef<Map<string, HTMLCanvasElement>>(new Map())

  const getSprite = useCallback((name: string, entity_type: string, r: number): HTMLCanvasElement => {
    const key = name === 'USER' ? `user_${Math.round(r)}` : `n_${entityHue(entity_type)}_${Math.round(r)}`
    let sprite = spriteCacheRef.current.get(key)
    if (!sprite) {
      const isUser = name === 'USER'
      const hue = isUser ? 45 : entityHue(entity_type)
      const sat = isUser ? 90 : 70
      const lit = isUser ? 70 : 65
      sprite = createNodeSprite(hue, sat, lit, r, isUser)
      spriteCacheRef.current.set(key, sprite)
    }
    return sprite
  }, [])

  // ── Cached adjacency (rebuilt only when edges change) ────────────
  const adjCacheRef = useRef<{ version: number; map: Map<string, string[]> }>({ version: -1, map: new Map() })

  const getAdjacency = useCallback((): Map<string, string[]> => {
    const edgeList = edgesRef.current
    if (adjCacheRef.current.version === edgeList.length) {
      return adjCacheRef.current.map
    }
    const adj = new Map<string, string[]>()
    for (const e of edgeList) {
      if (!adj.has(e.source)) adj.set(e.source, [])
      if (!adj.has(e.target)) adj.set(e.target, [])
      adj.get(e.source)!.push(e.target)
      adj.get(e.target)!.push(e.source)
    }
    adjCacheRef.current = { version: edgeList.length, map: adj }
    return adj
  }, [])

  // ── Position initialization ───────────────────────────────────────
  const ensurePositions = useCallback(() => {
    const pos = positionsRef.current
    const nodeList = nodesRef.current
    const edgeList = edgesRef.current

    const newNodes = nodeList.filter(n => !pos.has(n.name))
    if (newNodes.length === 0) {
      const names = new Set(nodeList.map(n => n.name))
      for (const key of pos.keys()) {
        if (!names.has(key)) pos.delete(key)
      }
      return
    }

    const isFullLoad = pos.size === 0 || newNodes.length > nodeList.length * 0.3

    if (isFullLoad) {
      pos.clear()

      const adj = new Map<string, string[]>()
      for (const n of nodeList) adj.set(n.name, [])
      for (const e of edgeList) {
        if (e.source !== 'USER' && e.target !== 'USER') {
          adj.get(e.source)?.push(e.target)
          adj.get(e.target)?.push(e.source)
        }
      }

      const visited = new Set<string>()
      const galaxies: string[][] = []
      for (const n of nodeList) {
        if (n.name === 'USER' || visited.has(n.name)) continue
        const comp: string[] = []
        const stack = [n.name]
        while (stack.length > 0) {
          const curr = stack.pop()!
          if (visited.has(curr)) continue
          visited.add(curr)
          comp.push(curr)
          for (const nb of adj.get(curr) || []) {
            if (!visited.has(nb) && nb !== 'USER') stack.push(nb)
          }
        }
        if (comp.length > 0) galaxies.push(comp)
      }

      pos.set('USER', { x: 0, y: 0, vx: 0, vy: 0, pinned: false })

      const galaxyRadius = 340
      const clusterRadius = 150
      for (let gi = 0; gi < galaxies.length; gi++) {
        const angle = (gi / Math.max(galaxies.length, 1)) * Math.PI * 2
        const gx = Math.cos(angle) * galaxyRadius
        const gy = Math.sin(angle) * galaxyRadius
        for (const name of galaxies[gi]) {
          const rng = pseudoRandom(hashString(name + ':' + gi))
          const la = rng() * Math.PI * 2
          const lr = 20 + rng() * clusterRadius
          pos.set(name, {
            x: gx + Math.cos(la) * lr,
            y: gy + Math.sin(la) * lr,
            vx: (rng() - 0.5) * 2,
            vy: (rng() - 0.5) * 2,
            pinned: false,
          })
        }
      }

      for (const n of nodeList) {
        if (!pos.has(n.name)) {
          const rng = pseudoRandom(hashString(n.name))
          const a = rng() * Math.PI * 2
          const r = 480 + rng() * 180
          pos.set(n.name, {
            x: Math.cos(a) * r,
            y: Math.sin(a) * r,
            vx: (rng() - 0.5) * 2,
            vy: (rng() - 0.5) * 2,
            pinned: false,
          })
        }
      }
    } else {
      for (const n of newNodes) {
        let cx = 0; let cy = 0; let count = 0
        for (const e of edgeList) {
          if (e.source === n.name) {
            const tp = pos.get(e.target); if (tp) { cx += tp.x; cy += tp.y; count++ }
          }
          if (e.target === n.name) {
            const sp = pos.get(e.source); if (sp) { cx += sp.x; cy += sp.y; count++ }
          }
        }
        const rng = pseudoRandom(hashString(n.name))
        if (count > 0) {
          cx /= count; cy /= count
          pos.set(n.name, {
            x: cx + (rng() - 0.5) * 60,
            y: cy + (rng() - 0.5) * 60,
            vx: (rng() - 0.5) * 2,
            vy: (rng() - 0.5) * 2,
            pinned: false,
          })
        } else {
          pos.set(n.name, {
            x: (rng() - 0.5) * 400,
            y: (rng() - 0.5) * 400,
            vx: (rng() - 0.5) * 2,
            vy: (rng() - 0.5) * 2,
            pinned: false,
          })
        }
      }
    }

    const names = new Set(nodeList.map(n => n.name))
    for (const key of pos.keys()) {
      if (!names.has(key)) pos.delete(key)
    }
  }, [])

  // ── Physics step with spatial hash ────────────────────────────────
  const stepPhysics = useCallback((): boolean => {
    const pos = positionsRef.current
    const nodeList = nodesRef.current
    if (nodeList.length === 0) return true

    const adj = getAdjacency()

    const grid = buildSpatialHash(nodeList, pos, PHYSICS.gridCellSize)

    for (let i = 0; i < nodeList.length; i++) {
      const a = nodeList[i]
      const pa = pos.get(a.name)
      if (!pa || pa.pinned) continue

      let fx = 0
      let fy = 0

      const cx = Math.floor(pa.x / PHYSICS.gridCellSize)
      const cy = Math.floor(pa.y / PHYSICS.gridCellSize)
      const neighborIndices = getNeighborIndices(grid, cx, cy)

      for (const j of neighborIndices) {
        if (i === j) continue
        const pb = pos.get(nodeList[j].name)
        if (!pb) continue
        const dx = pa.x - pb.x
        const dy = pa.y - pb.y
        const distSq = dx * dx + dy * dy
        if (distSq < 0.01) continue
        const dist = Math.sqrt(distSq)
        const force = Math.min(PHYSICS.repulsion / distSq, PHYSICS.maxForce)
        fx += (dx / dist) * force
        fy += (dy / dist) * force
      }

      const neighbors = adj.get(a.name) || []
      for (const nb of neighbors) {
        const pb = pos.get(nb)
        if (!pb) continue
        const dx = pb.x - pa.x
        const dy = pb.y - pa.y
        const dist = Math.sqrt(dx * dx + dy * dy) || 1
        const force = (dist - PHYSICS.springRest) * PHYSICS.springStrength
        fx += (dx / dist) * force
        fy += (dy / dist) * force
      }

      fx -= pa.x * PHYSICS.gravity
      fy -= pa.y * PHYSICS.gravity

      pa.vx = (pa.vx + fx) * PHYSICS.damping
      pa.vy = (pa.vy + fy) * PHYSICS.damping
      if (Math.abs(pa.vx) < 0.01) pa.vx = 0
      if (Math.abs(pa.vy) < 0.01) pa.vy = 0
    }

    for (const n of nodeList) {
      const p = pos.get(n.name)
      if (!p || p.pinned) continue
      p.x += p.vx
      p.y += p.vy
    }

    let totalKE = 0
    for (const n of nodeList) {
      const p = pos.get(n.name)
      if (!p || p.pinned) continue
      totalKE += p.vx * p.vx + p.vy * p.vy
    }

    return totalKE < PHYSICS.stopEnergy
  }, [getAdjacency])

  // ── Render frame ──────────────────────────────────────────────────
  const draw = useCallback(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const dpr = window.devicePixelRatio || 1
    const w = canvas.clientWidth
    const h = canvas.clientHeight
    if (w <= 0 || h <= 0) return
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr
      canvas.height = h * dpr
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    const tr = transformRef.current
    const nodeList = nodesRef.current
    const edgeList = edgesRef.current
    const pos = positionsRef.current
    const now = performance.now() * 0.001

    ctx.clearRect(0, 0, w, h)

    const cx = w / 2
    const cy = h / 2
    ctx.save()
    ctx.translate(tr.x + cx, tr.y + cy)
    ctx.scale(tr.scale, tr.scale)

    const invScale = 1 / tr.scale
    const margin = 80
    const vpMinX = -(tr.x + cx) * invScale - margin
    const vpMaxX = (w - tr.x - cx) * invScale + margin
    const vpMinY = -(tr.y + cy) * invScale - margin
    const vpMaxY = (h - tr.y - cy) * invScale + margin

    const nodeVisible = new Map<string, boolean>()
    for (const n of nodeList) {
      const p = pos.get(n.name)
      if (!p) { nodeVisible.set(n.name, false); continue }
      nodeVisible.set(n.name, p.x >= vpMinX && p.x <= vpMaxX && p.y >= vpMinY && p.y <= vpMaxY)
    }

    // Draw edges
    for (const e of edgeList) {
      const sp = pos.get(e.source)
      const tp = pos.get(e.target)
      if (!sp || !tp) continue

      if (!nodeVisible.get(e.source) && !nodeVisible.get(e.target)) continue

      ctx.beginPath()
      ctx.moveTo(sp.x, sp.y)
      ctx.lineTo(tp.x, tp.y)
      ctx.strokeStyle = 'hsla(220, 40%, 55%, 0.18)'
      ctx.lineWidth = 1.0 / tr.scale
      ctx.stroke()

      if (showLabels) {
        const mx = (sp.x + tp.x) / 2
        const my = (sp.y + tp.y) / 2
        const fontSize = Math.max(8, 10 / tr.scale)
        ctx.font = fontSize + 'px sans-serif'
        ctx.textAlign = 'center'
        ctx.textBaseline = 'bottom'
        const label = translateRel(e.relation_type)
        const tw = ctx.measureText(label).width
        const ph = fontSize * 1.4
        const pw = tw + fontSize * 0.8
        ctx.fillStyle = 'rgba(10,14,22,0.72)'
        const rx = mx - pw / 2
        const ry = my - ph - 2 / tr.scale
        ctx.beginPath()
        ctx.roundRect(rx, ry, pw, ph, ph / 2)
        ctx.fill()
        ctx.fillStyle = 'rgba(180,210,255,0.75)'
        ctx.fillText(label, mx, my - 2 / tr.scale)
      }
    }

    // Draw nodes
    for (const n of nodeList) {
      const p = pos.get(n.name)
      if (!p || !nodeVisible.get(n.name)) continue

      const isUser = n.name === 'USER'
      const baseR = isUser ? 20 : 6 + n.confidence * 10
      const hue = isUser ? 45 : entityHue(n.entity_type)
      const sat = isUser ? 90 : 70
      const lit = isUser ? 70 : 65

      if (isUser) {
        const pulse = 1 + Math.sin(now * 1.8) * 0.06
        const r = baseR * pulse
        ctx.save()
        ctx.translate(p.x, p.y)
        ctx.rotate(now * 0.2)
        for (let i = 0; i < 8; i++) {
          const ang = (i / 8) * Math.PI * 2
          const len = r * (2.2 + Math.sin(now * 1.5 + i) * 0.25)
          ctx.beginPath()
          ctx.moveTo(Math.cos(ang) * r * 1.3, Math.sin(ang) * r * 1.3)
          ctx.lineTo(Math.cos(ang) * len, Math.sin(ang) * len)
          ctx.lineWidth = r * 0.12
          ctx.strokeStyle = `hsla(${hue}, ${sat}%, ${lit}%, 0.28)`
          ctx.lineCap = 'round'
          ctx.stroke()
        }
        ctx.restore()

        ctx.beginPath()
        ctx.arc(p.x, p.y, r * 1.7, 0, Math.PI * 2)
        ctx.strokeStyle = `hsla(${hue}, ${sat}%, ${lit}%, 0.22)`
        ctx.lineWidth = 1 / tr.scale
        ctx.stroke()
      }

      const sprite = getSprite(n.name, n.entity_type, Math.round(baseR))
      const spriteSize = sprite.width
      const halfSize = spriteSize / 2
      ctx.drawImage(sprite, p.x - halfSize, p.y - halfSize, spriteSize, spriteSize)

      if (showLabels) {
        const fontSize = Math.max(9, 11 / tr.scale)
        ctx.font = (isUser ? 'bold ' : '') + fontSize + 'px sans-serif'
        ctx.textAlign = 'center'
        ctx.textBaseline = 'top'
        const labelY = p.y + baseR + 5 / tr.scale
        const textW = ctx.measureText(n.name).width
        const textH = fontSize * 1.2
        ctx.fillStyle = 'rgba(0,0,0,0.65)'
        ctx.fillRect(p.x - textW / 2 - 2, labelY - 1, textW + 4, textH + 2)
        ctx.fillStyle = `hsla(${hue}, 50%, 88%, 0.9)`
        ctx.fillText(n.name, p.x, labelY)
      }
    }

    ctx.restore()
  }, [showLabels, getSprite, translateRel])

  // ── Animation + physics loop ──────────────────────────────────────
  const stableFramesRef = useRef(0)
  const MAX_PHYSICS_FRAMES = 400

  useEffect(() => {
    let running = true
    let stable = false
    let frameCount = 0
    stableFramesRef.current = 0
    lastDrawTimeRef.current = 0

    const loop = (timestamp: number) => {
      if (!running) return
      ensurePositions()

      let physicsRan = false
      if (stableFramesRef.current < 20 && frameCount < MAX_PHYSICS_FRAMES) {
        stable = stepPhysics()
        physicsRan = true
        if (stable) {
          stableFramesRef.current++
        } else {
          stableFramesRef.current = 0
        }
      } else if (stableFramesRef.current < 20) {
        stable = true
        stableFramesRef.current = 20
      }

      const isIdle = stable && stableFramesRef.current >= 20 && !userActiveRef.current
      const timeSinceLastDraw = timestamp - lastDrawTimeRef.current

      if (isIdle) {
        if (timeSinceLastDraw >= IDLE_INTERVAL && needsDrawRef.current) {
          draw()
          lastDrawTimeRef.current = timestamp
          needsDrawRef.current = false
        }
      } else if (needsDrawRef.current || physicsRan || frameCount < 3 || userActiveRef.current) {
        draw()
        lastDrawTimeRef.current = timestamp
        needsDrawRef.current = false
      }

      frameCount++
      animRef.current = requestAnimationFrame(loop)
    }

    animRef.current = requestAnimationFrame(loop)
    return () => {
      running = false
      if (animRef.current) cancelAnimationFrame(animRef.current)
    }
  }, [ensurePositions, stepPhysics, draw])

  useEffect(() => {
    stableFramesRef.current = 0
    adjCacheRef.current.version = -1
    const pos = positionsRef.current
    spriteCacheRef.current.clear()
    for (const p of pos.values()) {
      p.vx += (hashString(p.x.toFixed(2)) / 2147483646 - 0.5) * 4
      p.vy += (hashString(p.y.toFixed(2)) / 2147483646 - 0.5) * 4
    }
    needsDrawRef.current = true
  }, [nodes.length, edges.length])

  // ── Hit test ──────────────────────────────────────────────────────
  const hitTest = useCallback((sx: number, sy: number): string | null => {
    const canvas = canvasRef.current
    if (!canvas) return null
    const tr = transformRef.current
    const w = canvas.clientWidth
    const h = canvas.clientHeight
    const cx = w / 2
    const cy = h / 2
    const wx = (sx - (tr.x + cx)) / tr.scale
    const wy = (sy - (tr.y + cy)) / tr.scale

    const pos = positionsRef.current
    const nodeList = nodesRef.current
    for (let i = nodeList.length - 1; i >= 0; i--) {
      const n = nodeList[i]
      const p = pos.get(n.name)
      if (!p) continue
      const r = n.name === 'USER' ? 22 : 8 + n.confidence * 14
      const hitR = Math.max(r * 1.5, 20)
      const dx = wx - p.x
      const dy = wy - p.y
      if (dx * dx + dy * dy < hitR * hitR) return n.name
    }
    return null
  }, [])

  const getCanvasPos = useCallback((e: React.MouseEvent) => {
    const canvas = canvasRef.current
    if (!canvas) return { x: 0, y: 0 }
    const rect = canvas.getBoundingClientRect()
    return {
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
    }
  }, [])

  // ── Mouse handlers ────────────────────────────────────────────────
  const lastHitTestRef = useRef(0)

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    userActiveRef.current = true
    const { x, y } = getCanvasPos(e)
    const hit = hitTest(x, y)
    const tr = transformRef.current

    if (hit) {
      const p = positionsRef.current.get(hit)
      if (p) p.pinned = true

      const neighborPins = new Map<string, { px: number; py: number }>()
      for (const e of edgesRef.current) {
        const nb = e.source === hit ? e.target : e.target === hit ? e.source : null
        if (nb) {
          const np = positionsRef.current.get(nb)
          if (np) {
            np.pinned = true
            neighborPins.set(nb, { px: np.x, py: np.y })
          }
        }
      }

      dragRef.current = {
        type: 'node', nodeName: hit,
        sx: x, sy: y,
        px: p?.x ?? 0, py: p?.y ?? 0,
        tx: tr.x, ty: tr.y,
        moved: false,
        neighborPins,
      }
    } else {
      dragRef.current = {
        type: 'pan',
        sx: x, sy: y,
        px: 0, py: 0,
        tx: tr.x, ty: tr.y,
        moved: false,
      }
    }
    needsDrawRef.current = true
  }, [getCanvasPos, hitTest])

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    const d = dragRef.current
    if (!d) {
      const now = performance.now()
      if (now - lastHitTestRef.current < 50) return
      lastHitTestRef.current = now

      const { x, y } = getCanvasPos(e)
      const hit = hitTest(x, y)
      if (canvasRef.current) {
        canvasRef.current.style.cursor = hit ? 'pointer' : 'grab'
      }
      return
    }

    const { x, y } = getCanvasPos(e)
    const dx = x - d.sx
    const dy = y - d.sy
    if (Math.abs(dx) > 2 || Math.abs(dy) > 2) d.moved = true

    if (d.type === 'pan') {
      transformRef.current = { ...transformRef.current, x: d.tx + dx, y: d.ty + dy }
    } else if (d.type === 'node' && d.nodeName) {
      const tr = transformRef.current
      const p = positionsRef.current.get(d.nodeName)
      if (p) {
        p.x = d.px + dx / tr.scale
        p.y = d.py + dy / tr.scale
      }
      if (d.neighborPins) {
        for (const [name, init] of d.neighborPins) {
          const np = positionsRef.current.get(name)
          if (np) {
            np.x = init.px + dx / tr.scale
            np.y = init.py + dy / tr.scale
          }
        }
      }
    }
    needsDrawRef.current = true
  }, [getCanvasPos, hitTest])

  const handleMouseUp = useCallback((e: React.MouseEvent) => {
    userActiveRef.current = false
    const d = dragRef.current

    if (d?.type === 'node' && d.nodeName) {
      const p = positionsRef.current.get(d.nodeName)
      if (p) { p.pinned = false; p.vx = 0; p.vy = 0 }
      if (d.neighborPins) {
        for (const name of d.neighborPins.keys()) {
          const np = positionsRef.current.get(name)
          if (np) { np.pinned = false; np.vx = 0; np.vy = 0 }
        }
      }
    }

    if (d && !d.moved) {
      if (d.type === 'node' && d.nodeName) {
        const nodeData = nodesRef.current.find(n => n.name === d.nodeName)
        if (nodeData) {
          onNodeClick({
            id: nodeData.name,
            name: nodeData.name,
            entity_type: nodeData.entity_type,
            description: nodeData.description,
            confidence: nodeData.confidence,
            scope: nodeData.scope,
            color: nodeData.name === 'USER' ? '#fbbf24' : entityColor(nodeData.entity_type),
          }, e.clientX, e.clientY)
        }
      } else if (d?.type === 'pan') {
        onBackgroundClick()
      }
    }

    dragRef.current = null
    needsDrawRef.current = true
  }, [onNodeClick, onBackgroundClick])

  // Native wheel listener
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      userActiveRef.current = true
      const delta = e.deltaY > 0 ? 0.9 : 1.1
      const tr = transformRef.current
      const newScale = Math.max(0.05, Math.min(20, tr.scale * delta))

      const rect = canvas.getBoundingClientRect()
      const mx = e.clientX - rect.left
      const my = e.clientY - rect.top
      const ratio = newScale / tr.scale
      transformRef.current = {
        x: mx - (mx - tr.x) * ratio,
        y: my - (my - tr.y) * ratio,
        scale: newScale,
      }
      needsDrawRef.current = true

      clearTimeout((canvas as any).__wheelTimeout)
      ;(canvas as any).__wheelTimeout = setTimeout(() => {
        userActiveRef.current = false
      }, 150)
    }

    canvas.addEventListener('wheel', onWheel, { passive: false })
    return () => {
      canvas.removeEventListener('wheel', onWheel)
      clearTimeout((canvas as any).__wheelTimeout)
    }
  }, [])

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
  }, [])

  return (
    <canvas
      ref={canvasRef}
      style={{
        width: '100%',
        height: '100%',
        display: 'block',
        cursor: 'grab',
        position: 'absolute',
        inset: 0,
        zIndex: 1,
      }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onContextMenu={handleContextMenu}
    />
  )
}
