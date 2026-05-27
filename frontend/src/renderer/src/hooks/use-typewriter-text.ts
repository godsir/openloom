// Typewriter text effect — progressive character reveal with grapheme awareness.
import { useState, useEffect, useRef } from 'react'
import { splitGraphemes } from '../utils/grapheme'

export function useTypewriterText(
  source: string,
  options?: { enabled?: boolean; speed?: number; batchSize?: number },
) {
  const { enabled = true, speed = 30 } = options || {}
  const [displayed, setDisplayed] = useState('')
  const rafRef = useRef<number>(0)
  const lastTime = useRef(0)

  useEffect(() => {
    if (!enabled) {
      setDisplayed(source)
      return
    }

    const graphemes = splitGraphemes(source)
    let index = 0
    lastTime.current = performance.now()

    const tick = (now: number) => {
      const elapsed = now - lastTime.current
      if (elapsed < speed) {
        rafRef.current = requestAnimationFrame(tick)
        return
      }
      lastTime.current = now

      // Batch: 1-2 when near real-time, 4-12 when catching up
      const remaining = graphemes.length - index
      const batchSize = remaining > 20 ? Math.min(12, Math.ceil(remaining / 4)) : 1

      index = Math.min(index + batchSize, graphemes.length)
      setDisplayed(graphemes.slice(0, index).join(''))

      if (index < graphemes.length) {
        rafRef.current = requestAnimationFrame(tick)
      }
    }

    rafRef.current = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(rafRef.current)
  }, [source, enabled, speed])

  const isComplete = displayed === source
  const tailGraphemes = splitGraphemes(source.slice(displayed.length)).slice(0, 6)

  return { displayed, isComplete, tailGraphemes }
}
