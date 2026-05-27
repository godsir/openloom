import { useRef, useEffect, useCallback } from 'react'

export function useSidebarResize(
  minWidth = 200,
  maxWidth = 480,
  storageKey = 'sidebar-width',
) {
  const sidebarRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const saved = localStorage.getItem(storageKey)
    if (saved && sidebarRef.current) {
      sidebarRef.current.style.width = `${saved}px`
    }
  }, [storageKey])

  const startResize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault()
      const el = sidebarRef.current
      if (!el) return

      const startX = e.clientX
      const startWidth = el.offsetWidth

      const onMove = (ev: MouseEvent) => {
        const delta = ev.clientX - startX
        const newWidth = Math.min(Math.max(startWidth + delta, minWidth), maxWidth)
        el.style.width = `${newWidth}px`
      }

      const onUp = () => {
        localStorage.setItem(storageKey, String(el.offsetWidth))
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
      }

      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
    },
    [minWidth, maxWidth, storageKey],
  )

  return { sidebarRef, startResize }
}
