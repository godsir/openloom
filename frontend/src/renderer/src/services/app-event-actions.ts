// App event action handlers — coordinate store updates on external events.
import { useStore } from '../stores'
import { loomRpc } from './jsonrpc'
import { scheduleSessionRefresh } from './session-refresh'

export function handleModelsChanged(): void {
  loomRpc('model.list')
    .then((r: any) => {
      const items = (r.models || []).filter((m: any) => m.name)
      useStore.getState().setModels(items)
      if (r.activeModel) useStore.getState().setCurrentModel(r.activeModel)
    })
    .catch(() => {})
}

export function handleAgentUpdated(): void {
  loomRpc('agent.config.list')
    .then((r: any) => useStore.getState().setAgents(r.configs || []))
    .catch(() => {})
}

export function handleThemeChanged(theme: string): void {
  useStore.getState().setTheme(theme as any)
}

export function handleSessionsChanged(): void {
  scheduleSessionRefresh(() => useStore.getState().loadSessions())
}
