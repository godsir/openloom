import { ipcMain, shell } from 'electron'

export function registerShellIpc(): void {
  ipcMain.handle('open-external', async (_, url: string) => {
    await shell.openExternal(url)
  })

  ipcMain.handle('open-folder', async (_, filePath: string) => {
    shell.showItemInFolder(filePath)
  })

  ipcMain.handle('open-file', async (_, filePath: string) => {
    await shell.openPath(filePath)
  })
}
