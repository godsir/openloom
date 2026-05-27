import { ipcMain, BrowserWindow, app } from 'electron'
import { getStoreKey, setStoreKey } from '../store'

export function registerAppIpc(): void {
  ipcMain.handle('get-platform', () => process.platform)

  ipcMain.handle('get-app-version', () => app.getVersion())

  ipcMain.handle('window-minimize', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize()
  })

  ipcMain.handle('window-maximize', (event) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    if (win?.isMaximized()) {
      win.unmaximize()
    } else {
      win?.maximize()
    }
  })

  ipcMain.handle('window-close', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close()
  })

  ipcMain.handle('window-is-maximized', (event) => {
    return BrowserWindow.fromWebContents(event.sender)?.isMaximized() ?? false
  })

  ipcMain.handle('get-preference', (_, key: string, fallback: unknown) => {
    return getStoreKey(key, fallback)
  })

  ipcMain.handle('set-preference', (_, key: string, value: unknown) => {
    setStoreKey(key, value)
  })
}
