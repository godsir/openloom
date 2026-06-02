import { useRef, useEffect, useCallback } from 'react'

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

const RELATION_ZH: Record<string, string> = {
  uses: '使用', works_on: '参与', knows: '了解',
  interested_in: '感兴趣', dislikes: '不喜欢', depends_on: '依赖',
  part_of: '属于', created_by: '创建者', related_to: '相关',
  has: '拥有', is_a: '是一种', belongs_to: '归属',
  connects_to: '连接', influences: '影响', produces: '产出',
  located_in: '位于', works_with: '协作', manages: '管理',
  reports_to: '汇报', mentions: '提及', prefers: '偏好',
}

function translateRel(rel: string): string {
  return RELATION_ZH[rel] ?? rel.replace(/_/g, ' ')
}

// Seeded PRNG for stable initial positions
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
  gravity: 0.00035,  // weak pull toward world origin — prevents isolated nodes from drifting away
}

export default function GalaxyGraph({ nodes, edges, showLabels, onNodeClick, onBackgroundClick }: GalaxyGraphProps) {
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
    /** neighbor name → initial world position when drag started */
    neighborPins?: Map<string, { px: number; py: number }>
  } | null>(null)
  const animRef = useRef<number>(0)
  const nodesRef = useRef(nodes)
  const edgesRef = useRef(edges)
  nodesRef.current = nodes
  edgesRef.current = edges

  // Initialize positions for new nodes — uses fixed world origin
  // (canvas transform handles centering, no dependency on canvas dimensions)
  const ensurePositions = useCallback(() => {
    const pos = positionsRef.current

    for (const n of nodesRef.current) {
      if (!pos.has(n.name)) {
        const seed = hashString(n.name)
        const rng = pseudoRandom(seed)
        const angle = rng() * Math.PI * 2
        const radius = 40 + rng() * 200
        pos.set(n.name, {
          x: Math.cos(angle) * radius,
          y: Math.sin(angle) * radius,
          vx: (rng() - 0.5) * 4,
          vy: (rng() - 0.5) * 4,
          pinned: false,
        })
      }
    }

    // Remove positions for nodes no longer in data
    const names = new Set(nodesRef.current.map(n => n.name))
    for (const key of pos.keys()) {
      if (!names.has(key)) pos.delete(key)
    }
  }, [])

  // Physics step — returns true if stable (low energy)
  const stepPhysics = useCallback((): boolean => {
    const pos = positionsRef.current
    const nodeList = nodesRef.current
    const edgeList = edgesRef.current
    if (nodeList.length === 0) return true

    // Build adjacency
    const adj = new Map<string, string[]>()
    for (const e of edgeList) {
      if (!adj.has(e.source)) adj.set(e.source, [])
      if (!adj.has(e.target)) adj.set(e.target, [])
      adj.get(e.source)!.push(e.target)
      adj.get(e.target)!.push(e.source)
    }

    // Apply forces
    for (let i = 0; i < nodeList.length; i++) {
      const a = nodeList[i]
      const pa = pos.get(a.name)
      if (!pa || pa.pinned) continue

      let fx = 0
      let fy = 0

      // Repulsion from all other nodes
      for (let j = 0; j < nodeList.length; j++) {
        if (i === j) continue
        const pb = pos.get(nodeList[j].name)
        if (!pb) continue
        const dx = pa.x - pb.x
        const dy = pa.y - pb.y
        const dist = Math.sqrt(dx * dx + dy * dy) || 1
        const force = Math.min(PHYSICS.repulsion / (dist * dist), PHYSICS.maxForce)
        fx += (dx / dist) * force
        fy += (dy / dist) * force
      }

      // Spring attraction along edges
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

      // Weak gravity toward world origin — gives isolated nodes an anchor
      fx -= pa.x * PHYSICS.gravity
      fy -= pa.y * PHYSICS.gravity

      pa.vx = (pa.vx + fx) * PHYSICS.damping
      pa.vy = (pa.vy + fy) * PHYSICS.damping
      // Snap tiny velocities to zero to guarantee eventual full stop
      if (Math.abs(pa.vx) < 0.01) pa.vx = 0
      if (Math.abs(pa.vy) < 0.01) pa.vy = 0
    }

    // Update positions and check energy
    let totalKE = 0
    for (const n of nodeList) {
      const p = pos.get(n.name)
      if (!p || p.pinned) continue
      p.x += p.vx
      p.y += p.vy
      totalKE += p.vx * p.vx + p.vy * p.vy
    }

    return totalKE < PHYSICS.stopEnergy
  }, [])

  // Render frame
  const draw = useCallback(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const dpr = window.devicePixelRatio || 1
    const w = canvas.clientWidth
    const h = canvas.clientHeight
    if (w <= 0 || h <= 0) return  // skip if canvas not laid out yet
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr
      canvas.height = h * dpr
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    const t = transformRef.current
    const nodeList = nodesRef.current
    const edgeList = edgesRef.current
    const pos = positionsRef.current
    const now = performance.now() * 0.001

    // Clear canvas (transparent — StarField behind provides background)
    ctx.clearRect(0, 0, w, h)

    // Transform: map world (0,0) to screen center
    const cx = w / 2
    const cy = h / 2
    ctx.save()
    ctx.translate(t.x + cx, t.y + cy)
    ctx.scale(t.scale, t.scale)

    // Draw edges
    for (const e of edgeList) {
      const sp = pos.get(e.source)
      const tp = pos.get(e.target)
      if (!sp || !tp) continue

      // Edge line with gradient from source color to target color
      const srcHue = e.source === 'USER' ? 45 : entityHue(e.source)
      const tgtHue = e.target === 'USER' ? 45 : entityHue(e.target)
      const grad = ctx.createLinearGradient(sp.x, sp.y, tp.x, tp.y)
      grad.addColorStop(0, `hsla(${srcHue}, 70%, 62%, 0.25)`)
      grad.addColorStop(1, `hsla(${tgtHue}, 70%, 62%, 0.25)`)
      ctx.beginPath()
      ctx.moveTo(sp.x, sp.y)
      ctx.lineTo(tp.x, tp.y)
      ctx.strokeStyle = grad
      ctx.lineWidth = 1.2 / t.scale
      ctx.stroke()

      if (showLabels) {
        const mx = (sp.x + tp.x) / 2
        const my = (sp.y + tp.y) / 2
        const fontSize = Math.max(8, 10 / t.scale)
        ctx.font = fontSize + 'px sans-serif'
        ctx.textAlign = 'center'
        ctx.textBaseline = 'bottom'
        const label = translateRel(e.relation_type)
        // pill background
        const tw = ctx.measureText(label).width
        const ph = fontSize * 1.4
        const pw = tw + fontSize * 0.8
        ctx.fillStyle = 'rgba(10,14,22,0.72)'
        const rx = mx - pw / 2
        const ry = my - ph - 2 / t.scale
        ctx.beginPath()
        ctx.roundRect(rx, ry, pw, ph, ph / 2)
        ctx.fill()
        ctx.fillStyle = 'rgba(180,210,255,0.75)'
        ctx.fillText(label, mx, my - 2 / t.scale)
      }
    }

    // Draw nodes
    for (const n of nodeList) {
      const p = pos.get(n.name)
      if (!p) continue

      const isUser = n.name === 'USER'
      const baseR = isUser ? 20 : 6 + n.confidence * 10
      const pulse = isUser ? 1 + Math.sin(now * 1.8) * 0.06 : 1
      const r = baseR * pulse
      const hue = isUser ? 45 : entityHue(n.entity_type)
      const sat = isUser ? 90 : 70
      const lit = isUser ? 70 : 65

      // USER sun: subtle rotating rays
      if (isUser) {
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

        // Thin orbit ring
        ctx.beginPath()
        ctx.arc(p.x, p.y, r * 1.7, 0, Math.PI * 2)
        ctx.strokeStyle = `hsla(${hue}, ${sat}%, ${lit}%, 0.22)`
        ctx.lineWidth = 1 / t.scale
        ctx.stroke()
      }

      // Soft bloom glow
      const bloomR = r * (isUser ? 4.0 : 3.0)
      const bloom = ctx.createRadialGradient(p.x, p.y, r * 0.2, p.x, p.y, bloomR)
      bloom.addColorStop(0,    `hsla(${hue}, ${sat}%, ${Math.min(lit + 18, 92)}%, 0.55)`)
      bloom.addColorStop(0.35, `hsla(${hue}, ${sat}%, ${lit}%, 0.18)`)
      bloom.addColorStop(1,    'rgba(0,0,0,0)')
      ctx.beginPath()
      ctx.arc(p.x, p.y, bloomR, 0, Math.PI * 2)
      ctx.fillStyle = bloom
      ctx.fill()

      // Flat solid disc
      ctx.beginPath()
      ctx.arc(p.x, p.y, r, 0, Math.PI * 2)
      ctx.fillStyle = `hsl(${hue}, ${sat}%, ${lit}%)`
      ctx.fill()

      // White-hot center
      const hotR = r * 0.5
      const hot = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, hotR)
      hot.addColorStop(0, 'rgba(255,255,255,0.92)')
      hot.addColorStop(0.5, 'rgba(255,255,255,0.35)')
      hot.addColorStop(1, 'rgba(255,255,255,0)')
      ctx.beginPath()
      ctx.arc(p.x, p.y, hotR, 0, Math.PI * 2)
      ctx.fillStyle = hot
      ctx.fill()

      // Label
      if (showLabels) {
        const fontSize = Math.max(9, 11 / t.scale)
        ctx.font = (isUser ? 'bold ' : '') + fontSize + 'px sans-serif'
        ctx.textAlign = 'center'
        ctx.textBaseline = 'top'
        ctx.shadowColor = 'rgba(0,0,0,0.9)'
        ctx.shadowBlur = 4 / t.scale
        ctx.fillStyle = `hsla(${hue}, 50%, 88%, 0.9)`
        ctx.fillText(n.name, p.x, p.y + r + 5 / t.scale)
        ctx.shadowBlur = 0
      }
    }

    ctx.restore()
  }, [showLabels])

  // Animation + physics loop
  useEffect(() => {
    let running = true
    let stable = false
    let frameCount = 0

    const loop = () => {
      if (!running) return
      ensurePositions()
      // Run physics for first ~300 frames, then only when unstable
      if (!stable || frameCount < 300) {
        stable = stepPhysics()
      }
      draw()
      frameCount++
      animRef.current = requestAnimationFrame(loop)
    }

    animRef.current = requestAnimationFrame(loop)
    return () => {
      running = false
      if (animRef.current) cancelAnimationFrame(animRef.current)
    }
  }, [ensurePositions, stepPhysics, draw])

  // Wake physics when graph data changes
  useEffect(() => {
    const pos = positionsRef.current
    for (const p of pos.values()) {
      p.vx += (hashString(p.x.toFixed(2)) / 2147483646 - 0.5) * 4
      p.vy += (hashString(p.y.toFixed(2)) / 2147483646 - 0.5) * 4
    }
  }, [nodes.length, edges.length])

  // Hit test: find node at screen coords
  const hitTest = useCallback((sx: number, sy: number): string | null => {
    const canvas = canvasRef.current
    if (!canvas) return null
    const t = transformRef.current
    const w = canvas.clientWidth
    const h = canvas.clientHeight
    const cx = w / 2
    const cy = h / 2
    const wx = (sx - (t.x + cx)) / t.scale
    const wy = (sy - (t.y + cy)) / t.scale

    const pos = positionsRef.current
    const nodeList = nodesRef.current
    for (let i = nodeList.length - 1; i >= 0; i--) {
      const n = nodeList[i]
      const p = pos.get(n.name)
      if (!p) continue
      const r = n.name === 'USER' ? 22 : 8 + n.confidence * 14
      const hitR = Math.max(r * 1.5, 20) // generous hit target matching visual core
      const dx = wx - p.x
      const dy = wy - p.y
      if (dx * dx + dy * dy < hitR * hitR) return n.name
    }
    return null
  }, [])

  const getCanvasPos = useCallback((e: React.MouseEvent) => {
    return { x: e.nativeEvent.offsetX, y: e.nativeEvent.offsetY }
  }, [])

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    const { x, y } = getCanvasPos(e)
    const hit = hitTest(x, y)
    const t = transformRef.current

    if (hit) {
      const p = positionsRef.current.get(hit)
      if (p) p.pinned = true

      // Collect direct neighbors from edges and pin them too
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
        tx: t.x, ty: t.y,
        moved: false,
        neighborPins,
      }
    } else {
      dragRef.current = {
        type: 'pan',
        sx: x, sy: y,
        px: 0, py: 0,
        tx: t.x, ty: t.y,
        moved: false,
      }
    }
  }, [getCanvasPos, hitTest])

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    const d = dragRef.current
    if (!d) {
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
      const t = transformRef.current
      const p = positionsRef.current.get(d.nodeName)
      if (p) {
        p.x = d.px + dx / t.scale
        p.y = d.py + dy / t.scale
      }
      // Move neighbors along with the dragged node
      if (d.neighborPins) {
        for (const [name, init] of d.neighborPins) {
          const np = positionsRef.current.get(name)
          if (np) {
            np.x = init.px + dx / t.scale
            np.y = init.py + dy / t.scale
          }
        }
      }
    }
  }, [getCanvasPos, hitTest])

  const handleMouseUp = useCallback((e: React.MouseEvent) => {
    const d = dragRef.current

    if (d?.type === 'node' && d.nodeName) {
      const p = positionsRef.current.get(d.nodeName)
      if (p) {
        p.pinned = false
        p.vx = 0
        p.vy = 0
      }
      // Unpin neighbors and zero their velocity
      if (d.neighborPins) {
        for (const name of d.neighborPins.keys()) {
          const np = positionsRef.current.get(name)
          if (np) {
            np.pinned = false
            np.vx = 0
            np.vy = 0
          }
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
  }, [onNodeClick, onBackgroundClick])

  // Native wheel listener (must be non-passive for preventDefault)
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const delta = e.deltaY > 0 ? 0.9 : 1.1
      const t = transformRef.current
      const newScale = Math.max(0.05, Math.min(20, t.scale * delta))

      const rect = canvas.getBoundingClientRect()
      const mx = e.clientX - rect.left
      const my = e.clientY - rect.top
      const ratio = newScale / t.scale
      transformRef.current = {
        x: mx - (mx - t.x) * ratio,
        y: my - (my - t.y) * ratio,
        scale: newScale,
      }
    }

    canvas.addEventListener('wheel', onWheel, { passive: false })
    return () => canvas.removeEventListener('wheel', onWheel)
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
