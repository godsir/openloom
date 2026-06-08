import { ipcMain, dialog } from 'electron'
import { readFileSync } from 'fs'

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
      const full = readFileSync(filePath, 'utf-8')
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
