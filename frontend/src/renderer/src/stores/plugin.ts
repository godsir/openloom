import { StateCreator } from 'zustand'
import { loomRpc } from '../services/jsonrpc'

// --- Plugin types (shared between store and UI) ---

export interface HookHandlerInfo {
  type: string  // 'command' | 'prompt' | 'agent'
  command?: string
  prompt?: string
  timeout: number
  matcher?: string
}

export interface HookEventDetail {
  event: string
  handler_count: number
  handlers: HookHandlerInfo[]
}

export interface PluginInfo {
  name: string
  version?: string
  description?: string
  path?: string
  source: string
  skill_count?: number
  hook_count?: number
  mcp_server_count?: number
  has_settings: boolean
  skills?: Array<{name: string, path?: string}>
  mcp_servers?: Array<{name: string, transport: string}>
  hooks?: HookEventDetail[]
}

// --- Slice ---

export interface PluginSlice {
  /** Cached plugin list — read from cache on every access. */
  plugins: PluginInfo[]
  /** Whether the initial cache load has completed. */
  pluginsLoaded: boolean
  /** Replace the entire cached plugin list. */
  setPlugins: (plugins: PluginInfo[]) => void
  /** Populate the cache by calling plugins.list on the backend. */
  loadPlugins: () => Promise<void>
  /** Trigger a backend rescan (plugins.reload), then refresh the cache. */
  reloadPlugins: () => Promise<void>
}

export const createPluginSlice: StateCreator<PluginSlice> = (set) => ({
  plugins: [],
  pluginsLoaded: false,

  setPlugins: (plugins) => set({ plugins, pluginsLoaded: true }),

  loadPlugins: async () => {
    try {
      const res = await loomRpc<{ plugins: PluginInfo[] }>('plugins.list')
      set({ plugins: res.plugins ?? [], pluginsLoaded: true })
    } catch {
      // Keep stale data, mark as loaded to unblock UI retries
      set((s) => ({ pluginsLoaded: true }))
    }
  },

  reloadPlugins: async () => {
    try {
      await loomRpc('plugins.reload')
    } catch {
      // reload best-effort — proceed to refresh even on failure
    }
    try {
      const res = await loomRpc<{ plugins: PluginInfo[] }>('plugins.list')
      set({ plugins: res.plugins ?? [], pluginsLoaded: true })
    } catch {
      // keep previous cache on refresh failure
    }
  },
})
