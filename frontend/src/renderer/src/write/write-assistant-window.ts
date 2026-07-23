type WindowOpen = (
  url?: string | URL,
  target?: string,
  features?: string,
) => Window | null

export function openWriteAssistantWindow(open: WindowOpen = window.open.bind(window)): Window | null {
  return open(
    'about:blank',
    'openloom-write-assistant',
    'popup=yes,width=380,height=640,resizable=yes',
  )
}

