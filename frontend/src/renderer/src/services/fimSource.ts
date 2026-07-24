import { CompletionContext } from '@codemirror/autocomplete'
import { useStore } from '../stores'
import { useWriteStore } from '../stores/write'
import { requestFimCompletion } from './completion'

/** Debounce helper: last-write-wins, with a leading edge check */
function createDebouncer() {
  let timerId: ReturnType<typeof setTimeout> | null = null
  let lastResolve: ((v: string | null) => void) | null = null
  let requestSeq = 0

  return function debouncedRun(prefix: string, suffix: string, maxTokens: number, delayMs: number): Promise<string | null> {
    const seq = ++requestSeq
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
          resolve(seq === requestSeq && result.ok && result.completion ? result.completion : null)
        } catch {
          resolve(null)
        }
      }, delayMs)
    })
  }
}

/**
 * Shared FIM (Fill-in-the-Middle) completion source.
 * Works in both 'chat' and 'write' app modes.
 *
 * Triggered automatically after a short typing pause (debounced),
 * or manually via Ctrl+Space.
 */
export function buildFimCompletionSource() {
  const debouncedFim = createDebouncer()
  return async (context: CompletionContext) => {
    const state = useStore.getState()
    const appMode = state.appMode
    // Only active in chat and write modes
    if (appMode !== 'chat' && appMode !== 'write') return null

    // Skip if not explicit and prefix is too short
    const view = context.view
    if (!view) return null
    const pos = context.pos
    const doc = view.state.doc.toString()
    const prefix = doc.slice(Math.max(0, pos - 8_000), pos)
    const suffix = doc.slice(pos, Math.min(doc.length, pos + 4_000))

    if (prefix.length < 10 && !context.explicit) return null
    if (prefix.length < 3) return null // never complete for empty/almost-empty

    try {
      const writeConfig = useWriteStore.getState()
      const maxTokens = appMode === 'write' ? writeConfig.shortMaxTokens : 64
      const delayMs = appMode === 'write' ? writeConfig.shortDebounceMs : 500
      const text = await debouncedFim(prefix, suffix, maxTokens, delayMs)
      if (context.aborted) return null
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
