import { CompletionContext } from '@codemirror/autocomplete'
import { useStore } from '../stores'
import { requestFimCompletion } from './completion'

/** Debounce helper: last-write-wins, with a leading edge check */
function createDebouncer(delayMs: number) {
  let timerId: ReturnType<typeof setTimeout> | null = null
  let lastResolve: ((v: string | null) => void) | null = null
  let pendingCount = 0

  return function debouncedRun(prefix: string, suffix: string, maxTokens: number): Promise<string | null> {
    pendingCount++
    return new Promise((resolve) => {
      // Cancel previous pending call
      if (timerId !== null) {
        clearTimeout(timerId)
        lastResolve?.(null)
      }
      lastResolve = resolve
      timerId = setTimeout(async () => {
        timerId = null
        try {
          const result = await requestFimCompletion(prefix, suffix, maxTokens)
          resolve(result.ok && result.completion ? result.completion.trim() : null)
        } catch {
          resolve(null)
        }
      }, delayMs)
    })
  }
}

const debouncedFim = createDebouncer(500)

/**
 * Shared FIM (Fill-in-the-Middle) completion source.
 * Works in both 'chat' and 'write' app modes.
 *
 * Triggered automatically after a short typing pause (debounced),
 * or manually via Ctrl+Space.
 */
export function buildFimCompletionSource() {
  return async (context: CompletionContext) => {
    const state = useStore.getState()
    const appMode = state.appMode
    // Only active in chat and write modes
    if (appMode !== 'chat' && appMode !== 'write') return null

    // Skip if not explicit and prefix is too short
    const view = context.view
    const pos = context.pos
    const doc = view.state.doc.toString()
    const prefix = doc.slice(0, pos)
    const suffix = doc.slice(pos)

    if (prefix.length < 10 && !context.explicit) return null
    if (prefix.length < 3) return null // never complete for empty/almost-empty

    try {
      const text = await debouncedFim(prefix, suffix, 64)
      if (!text || text.length === 0) return null
      return {
        from: pos,
        to: pos,
        options: [{
          label: text,
          type: 'text',
          apply: text,
        }],
        filter: false,
      }
    } catch { /* silent */ }
    return null
  }
}
