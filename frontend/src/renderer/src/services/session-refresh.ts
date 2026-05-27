// Debounced session list refresh to avoid duplicate in-flight requests.
let pendingRefresh: ReturnType<typeof setTimeout> | null = null
let refreshing = false
const COOLDOWN = 250

export function scheduleSessionRefresh(refreshFn: () => Promise<void>): void {
  if (pendingRefresh) clearTimeout(pendingRefresh)
  pendingRefresh = setTimeout(async () => {
    if (refreshing) return
    refreshing = true
    try {
      await refreshFn()
    } catch {
      // Silently ignore
    } finally {
      refreshing = false
    }
  }, COOLDOWN)
}
