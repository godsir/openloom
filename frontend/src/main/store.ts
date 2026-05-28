import { join } from 'path'
import { homedir } from 'os'
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'

const storeDir = join(homedir(), '.loom')
const storePath = join(storeDir, 'preferences.json')

export function readStore(): Record<string, unknown> {
  try {
    if (!existsSync(storePath)) return {}
    return JSON.parse(readFileSync(storePath, 'utf-8'))
  } catch {
    return {}
  }
}

export function writeStore(data: Record<string, unknown>): void {
  if (!existsSync(storeDir)) mkdirSync(storeDir, { recursive: true })
  writeFileSync(storePath, JSON.stringify(data, null, 2), 'utf-8')
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
