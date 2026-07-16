import { join } from 'path'
import { homedir } from 'os'
import { readFileSync, writeFileSync, existsSync, mkdirSync, renameSync } from 'fs'

const storeDir = join(homedir(), '.loom')
const configPath = join(storeDir, 'config.json')
const legacyPath = join(storeDir, 'preferences.json')

interface UnifiedConfig {
  version?: number
  preferences?: Record<string, unknown>
  [key: string]: unknown
}

function readConfig(): UnifiedConfig {
  try {
    if (!existsSync(configPath)) return {}
    return JSON.parse(readFileSync(configPath, 'utf-8'))
  } catch {
    return {}
  }
}

function writeConfig(config: UnifiedConfig): void {
  if (!existsSync(storeDir)) mkdirSync(storeDir, { recursive: true })
  // Atomic write (tmp + rename): a crash mid-write can never leave a
  // half-written config.json, and the window for cross-process races with
  // the backend ConfigStore is minimised.
  // NOTE: this does NOT fully eliminate the read-modify-write race — if the
  // backend writes another section between our read and rename, that change
  // is lost. Fully fixing that needs preferences writes to go through a
  // backend RPC so only the backend touches config.json.
  const tmp = `${configPath}.tmp`
  writeFileSync(tmp, JSON.stringify(config, null, 2), 'utf-8')
  renameSync(tmp, configPath)
}

export function readStore(): Record<string, unknown> {
  const config = readConfig()
  if (config.preferences && typeof config.preferences === 'object') {
    return config.preferences as Record<string, unknown>
  }
  // Fallback: legacy preferences.json
  try {
    if (existsSync(legacyPath)) {
      return JSON.parse(readFileSync(legacyPath, 'utf-8'))
    }
  } catch {
    // ignore
  }
  return {}
}

/** Read the tool_prefs section from ~/.loom/config.json (e.g. proxy settings). */
export function readToolPrefs(): Record<string, unknown> {
  const config = readConfig()
  const prefs = config.tool_prefs
  return prefs && typeof prefs === 'object' ? (prefs as Record<string, unknown>) : {}
}

export function writeStore(data: Record<string, unknown>): void {
  const config = readConfig()
  config.preferences = data
  writeConfig(config)
}

export function getStoreKey<T>(key: string, fallback: T): T {
  const data = readStore()
  return (key in data) ? data[key] as T : fallback
}

export function setStoreKey(key: string, value: unknown): void {
  const data = readStore()
  data[key] = value
  writeStore(data)
}
