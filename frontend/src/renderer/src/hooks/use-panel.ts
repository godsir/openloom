import { useState, useCallback } from 'react'

export function usePanel(initialPanel: string | null = null) {
  const [activePanel, setActivePanel] = useState<string | null>(initialPanel)

  const open = useCallback((panel: string) => setActivePanel(panel), [])
  const close = useCallback(() => setActivePanel(null), [])
  const toggle = useCallback(
    (panel: string) => setActivePanel((prev) => (prev === panel ? null : panel)),
    [],
  )

  return { activePanel, open, close, toggle }
}
