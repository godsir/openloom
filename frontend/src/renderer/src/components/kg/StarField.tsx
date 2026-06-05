import { useRef, useEffect } from 'react'

// Seeded PRNG for stable star field layout
function mulberry32(seed: number) {
  return () => {
    seed |= 0; seed = seed + 0x6D2B79F5 | 0
    let t = Math.imul(seed ^ seed >>> 15, 1 | seed)
    t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t
    return ((t ^ t >>> 14) >>> 0) / 4294967296
  }
}

interface StarFieldProps {
  width: number
  height: number
  className?: string
}

// Target ~20 fps for background stars to reduce GPU competition with GalaxyGraph
const STARFIELD_FPS = 20
const FRAME_INTERVAL = 1000 / STARFIELD_FPS

export default function StarField({ width, height, className }: StarFieldProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const lastDrawRef = useRef<number>(0)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    if (width < 10 || height < 10) return

    const dpr = window.devicePixelRatio || 1
    canvas.width = width * dpr
    canvas.height = height * dpr
    canvas.style.width = width + 'px'
    canvas.style.height = height + 'px'
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.scale(dpr, dpr)

    // Generate star data — reduced density from /400 to /800
    const rng = mulberry32(42)
    const starCount = Math.floor((width * height) / 800)
    const stars: { x: number; y: number; r: number; baseBrightness: number; phase: number; speed: number }[] = []
    for (let i = 0; i < starCount; i++) {
      stars.push({
        x: rng() * width,
        y: rng() * height,
        r: rng() * 1.2 + 0.2,
        baseBrightness: rng() * 0.5 + 0.1,
        phase: rng() * Math.PI * 2,
        speed: rng() * 0.002 + 0.001,
      })
    }

    // Generate bright stars with flares — only 1% of total (reduced from 2%)
    const brightStars: { x: number; y: number; r: number; baseBrightness: number; phase: number; speed: number }[] = []
    for (let i = 0; i < Math.floor(starCount * 0.01); i++) {
      brightStars.push({
        x: rng() * width,
        y: rng() * height,
        r: rng() * 0.8 + 0.5,
        baseBrightness: rng() * 0.3 + 0.5,
        phase: rng() * Math.PI * 2,
        speed: rng() * 0.003 + 0.002,
      })
    }

    // Nebula data — reduced from 6 to 4
    const nebulaColors = [
      'rgba(34, 211, 238, 0.012)',
      'rgba(167, 139, 250, 0.010)',
      'rgba(244, 114, 182, 0.008)',
      'rgba(52, 211, 153, 0.008)',
    ]
    const nebulae: { x: number; y: number; r: number; color: string; phase: number; speed: number }[] = []
    for (let i = 0; i < 4; i++) {
      nebulae.push({
        x: rng() * width,
        y: rng() * height,
        r: rng() * Math.min(width, height) * 0.4 + 80,
        color: nebulaColors[i % nebulaColors.length],
        phase: rng() * Math.PI * 2,
        speed: rng() * 0.0005 + 0.0002,
      })
    }

    // ── Pre-render static background layer ──────────────────────────
    // The deep space gradient and nebulae don't need per-frame updates.
    // We render them once to an offscreen canvas, then blit it each frame.
    const staticBg = document.createElement('canvas')
    staticBg.width = width
    staticBg.height = height
    const bgCtx = staticBg.getContext('2d')!
    const maxDim = Math.max(width, height)
    const bgGrad = bgCtx.createRadialGradient(width / 2, height / 2, 0, width / 2, height / 2, maxDim * 0.7)
    bgGrad.addColorStop(0, '#0d1117')
    bgGrad.addColorStop(0.5, '#080b10')
    bgGrad.addColorStop(1, '#050709')
    bgCtx.fillStyle = bgGrad
    bgCtx.fillRect(0, 0, width, height)

    for (const neb of nebulae) {
      const grad = bgCtx.createRadialGradient(neb.x, neb.y, 0, neb.x, neb.y, neb.r)
      grad.addColorStop(0, neb.color)
      grad.addColorStop(1, 'transparent')
      bgCtx.fillStyle = grad
      bgCtx.fillRect(0, 0, width, height)
    }

    // Also pre-render non-twinkling dim stars onto the static bg
    for (const star of stars) {
      bgCtx.beginPath()
      bgCtx.arc(star.x, star.y, star.r, 0, Math.PI * 2)
      bgCtx.fillStyle = `rgba(255,255,255,${star.baseBrightness * 0.7})`
      bgCtx.fill()
    }

    let animationId: number
    const draw = (timestamp: number) => {
      // Throttle to ~20fps
      if (timestamp - lastDrawRef.current < FRAME_INTERVAL) {
        animationId = requestAnimationFrame(draw)
        return
      }
      lastDrawRef.current = timestamp

      const time = Date.now()

      // Blit static background (deep space + nebulae + dim stars)
      ctx.clearRect(0, 0, width, height)
      ctx.drawImage(staticBg, 0, 0)

      // Animated twinkling overlay for stars — only vary brightness on top of static
      for (const star of stars) {
        const twinkle = Math.sin(time * star.speed + star.phase) * 0.3
        if (Math.abs(twinkle) < 0.05) continue // skip negligible changes
        ctx.beginPath()
        ctx.arc(star.x, star.y, star.r, 0, Math.PI * 2)
        ctx.fillStyle = `rgba(255,255,255,${star.baseBrightness * twinkle * 0.5})`
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

    animationId = requestAnimationFrame(draw)
    return () => {
      if (animationId) cancelAnimationFrame(animationId)
    }
  }, [width, height])

  return (
    <canvas
      ref={canvasRef}
      className={className}
    />
  )
}
