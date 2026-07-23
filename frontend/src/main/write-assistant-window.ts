export const WRITE_ASSISTANT_WINDOW_NAME = 'openloom-write-assistant'

export function isWriteAssistantWindowRequest(url: string, frameName: string): boolean {
  return url === 'about:blank' && frameName === WRITE_ASSISTANT_WINDOW_NAME
}

export function isSafeExternalWindowUrl(url: string): boolean {
  try {
    return ['http:', 'https:', 'mailto:'].includes(new URL(url).protocol)
  } catch {
    return false
  }
}
