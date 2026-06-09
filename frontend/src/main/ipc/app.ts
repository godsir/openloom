import { ipcMain, BrowserWindow, app, Notification } from 'electron'
import * as path from 'path'
import { getStoreKey, setStoreKey } from '../store'
import { checkForUpdates, downloadUpdate, installUpdate } from '../updater'
import { restartEngine } from '../engine'

export function registerAppIpc(): void {
  ipcMain.handle('get-loom-dir', () => {
    const home = process.env.USERPROFILE || process.env.HOME || ''
    return path.join(home, '.loom')
  })
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
    const closeToTray = getStoreKey('closeToTray', true)
    const win = BrowserWindow.fromWebContents(event.sender)
    if (closeToTray) {
      win?.hide()
    } else {
      win?.close()
    }
  })

  ipcMain.handle('window-is-maximized', (event) => {
    return BrowserWindow.fromWebContents(event.sender)?.isMaximized() ?? false
  })

  ipcMain.handle('get-preference', (_, key: string, fallback: unknown) => {
    return getStoreKey(key, fallback)
  })

  ipcMain.handle('set-preference', (_, key: string, value: unknown) => {
    setStoreKey(key, value)
    // Apply auto-start immediately
    if (key === 'autoStart') {
      app.setLoginItemSettings({ openAtLogin: !!value })
    }
  })

  // Auto-update
  ipcMain.handle('check-for-updates', async () => {
    await checkForUpdates()
  })

  ipcMain.handle('download-update', async () => {
    await downloadUpdate()
  })

  ipcMain.handle('install-update', () => {
    installUpdate()
  })

  // Engine restart
  ipcMain.handle('engine:restart', async () => {
    return restartEngine()
  })

  // Native OS notification
  ipcMain.handle('show-notification', (_, title: string, body: string) => {
    if (!Notification.isSupported()) return
    const n = new Notification({ title, body })
    n.show()
  })
}
