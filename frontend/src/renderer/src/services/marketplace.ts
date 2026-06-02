// Marketplace API service — typed wrappers around loomRpc for marketplace calls.

import { loomRpc } from './jsonrpc'

/** A marketplace entry — can be a plugin or a skill. */
export interface MarketPlugin {
  id: string
  name: string
  description: string
  version: string
  author: string
  git_url: string
  category: string
  /** Entry kind: "plugin" or "skill". */
  kind: string
  tags: string[]
  homepage?: string
  installed: boolean
  /** Whether a newer version is available in the catalog. */
  has_update: boolean
  installed_version?: string
  installed_path?: string
}

/** List all entries in the marketplace catalog with install status. */
export function listMarketplace(): Promise<{ plugins: MarketPlugin[] }> {
  return loomRpc<{ plugins: MarketPlugin[] }>('marketplace.list')
}

/** Install an entry from the marketplace by its catalog ID. */
export function installMarketPlugin(pluginId: string): Promise<{ ok: boolean; path: string }> {
  return loomRpc('marketplace.install', { plugin_id: pluginId })
}

/** Uninstall a marketplace entry by its catalog ID. */
export function uninstallMarketPlugin(pluginId: string): Promise<{ ok: boolean }> {
  return loomRpc('marketplace.uninstall', { plugin_id: pluginId })
}

/** Update an installed marketplace entry to the latest version. */
export function updateMarketPlugin(pluginId: string): Promise<{ ok: boolean }> {
  return loomRpc('marketplace.update', { plugin_id: pluginId })
}
