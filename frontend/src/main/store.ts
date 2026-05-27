import { app } from 'electron'
import { join } from 'path'
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'

const storePath = join(app.getPath('userData'), 'preferences.json')

export function readStore(): Record<string, unknown> {
  try {
    if (!existsSync(storePath)) return {}
    return JSON.parse(readFileSync(storePath, 'utf-8'))
  } catch {
    return {}
  }
}

export function writeStore(data: Record<string, unknown>): void {
  const dir = app.getPath('userData')
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
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
