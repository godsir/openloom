import { ipcMain, dialog } from 'electron'
import { readFileSync } from 'fs'
import { resolve, join, sep } from 'path'
import { homedir } from 'os'

// The app's private data dir holds secrets (credentials.json) and the SQLite
// databases — never expose it through the renderer-facing read-file API.
const LOOM_DATA_DIR = resolve(join(homedir(), '.loom'))

export function registerFileIpc(): void {
  ipcMain.handle('select-folder', async () => {
    const result = await dialog.showOpenDialog({ properties: ['openDirectory'] })
    return result.canceled ? null : result.filePaths[0]
  })

  ipcMain.handle('select-files', async (_, options?: { filters?: { name: string; extensions: string[] }[] }) => {
    const result = await dialog.showOpenDialog({
      properties: ['openFile', 'multiSelections'],
      filters: options?.filters,
    })
    return result.canceled ? [] : result.filePaths
  })

  ipcMain.handle('read-file', async (_event, filePath: string, options?: {
    startLine?: number   // 1-based, inclusive
    endLine?: number     // 1-based, inclusive
  }) => {
    try {
      if (typeof filePath !== 'string' || !filePath) return null
      const resolved = resolve(filePath)
      if (resolved === LOOM_DATA_DIR || resolved.startsWith(LOOM_DATA_DIR + sep)) {
        return null
      }
      const full = readFileSync(resolved, 'utf-8')
      if (!options || (options.startLine == null && options.endLine == null)) {
        return full
      }
      const lines = full.split('\n')
      const start = Math.max(0, (options.startLine ?? 1) - 1)
      const end = Math.min(lines.length, (options.endLine ?? lines.length))
      return lines.slice(start, end).join('\n')
    } catch {
      return null
    }
  })
}
