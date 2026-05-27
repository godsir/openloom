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

  ipcMain.handle('read-file', async (_, filePath: string) => {
    try {
      return readFileSync(filePath, 'utf-8')
    } catch {
      return null
    }
  })
}
