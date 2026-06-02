import { readdirSync, statSync } from 'fs'
import { join } from 'path'
import { homedir } from 'os'
import { BrowserWindow } from 'electron'

const LOOM_DIR = join(homedir(), '.loom')
const POLL_INTERVAL_MS = 30_000

/** Recursively walk a directory and collect all file (path, mtimeMs) entries. */
function collectFiles(dir: string, prefix: string, out: Map<string, number>): void {
  let entries: string[]
  try {
    entries = readdirSync(dir)
  } catch {
    return
  }
  for (const name of entries) {
    const full = join(dir, name)
    const key = prefix + name
    let st
    try {
      st = statSync(full)
    } catch {
      continue
    }
    if (st.isFile()) {
      out.set(key, st.mtimeMs)
    } else if (st.isDirectory()) {
      collectFiles(full, key + '/', out)
    }
  }
}

function snapshotFiles(root: string): Map<string, number> {
  const files = new Map<string, number>()
  collectFiles(root, '', files)
  return files
}

function hasChanged(prev: Map<string, number>, current: Map<string, number>): boolean {
  if (prev.size !== current.size) return true
  for (const [path, mtime] of current) {
    if (prev.get(path) !== mtime) return true
  }
  return false
}

let pollTimer: ReturnType<typeof setInterval> | null = null
let lastSnapshot: Map<string, number> | null = null

export function startConfigWatcher(): void {
  if (pollTimer) return // already running

  // Take an initial snapshot so the first poll doesn't trigger false positive
  lastSnapshot = snapshotFiles(LOOM_DIR)

  pollTimer = setInterval(() => {
    try {
      const current = snapshotFiles(LOOM_DIR)
      if (lastSnapshot && hasChanged(lastSnapshot, current)) {
        // Notify all open windows about model config change
        const wins = BrowserWindow.getAllWindows()
        for (const win of wins) {
          win.webContents.send('model-config-changed')
        }
      }
      lastSnapshot = current
    } catch {
      // best-effort; skip a cycle on error
    }
  }, POLL_INTERVAL_MS)

  console.log(`[config-watcher] polling ${LOOM_DIR} every ${POLL_INTERVAL_MS / 1000}s`)
}

export function stopConfigWatcher(): void {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
    lastSnapshot = null
  }
}
