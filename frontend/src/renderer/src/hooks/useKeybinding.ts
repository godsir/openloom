import { useEffect } from 'react'

/**
 * Register a temporary keybinding that only fires while this component is mounted.
 * The handler receives DOM keydown events. Use for local shortcuts like modal close.
 *
 * Keys format: "ctrl+n", "escape", etc.
 */
export function useKeybinding(
  keys: string | string[],
  handler: (e: KeyboardEvent) => void,
  deps: unknown[] = [],
): void {
  const keySet = new Set(Array.isArray(keys) ? keys : [keys])

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const parts: string[] = []
      if (e.ctrlKey || e.metaKey) parts.push('ctrl')
      if (e.altKey) parts.push('alt')
      if (e.shiftKey) parts.push('shift')
      const key = e.key.toLowerCase()
      if (['control', 'alt', 'shift', 'meta'].includes(key)) return
      parts.push(key)
      const combo = parts.join('+')

      if (keySet.has(combo) || keySet.has(key)) {
        e.preventDefault()
        e.stopPropagation()
        handler(e)
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [keySet, ...deps])
}
