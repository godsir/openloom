import { ipcMain, BrowserWindow, app, Notification } from 'electron'
import * as path from 'path'
import { getStoreKey, setStoreKey } from '../store'
import { checkForUpdates, configureUpdaterProxy, downloadUpdate, cancelDownloadUpdate, installUpdate, getUpdateChannel, setUpdateChannel } from '../updater'
import { restartEngine } from '../engine'
import { getGitBranches, switchGitBranch, createAndSwitchGitBranch, getUncommittedChanges, gitCommit, gitPush } from '../services/git-service'

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
    // Apply auto-start immediately (only in packaged/production builds)
    if (key === 'autoStart' && app.isPackaged) {
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

  ipcMain.handle('cancel-download-update', () => {
    cancelDownloadUpdate()
  })

  ipcMain.handle('refresh-updater-proxy', async () => {
    await configureUpdaterProxy()
  })

  ipcMain.handle('install-update', () => {
    installUpdate()
  })

  ipcMain.handle('get-update-channel', () => {
    return getUpdateChannel()
  })

  ipcMain.handle('set-update-channel', (_event, channel: string) => {
    setUpdateChannel(channel as 'stable' | 'beta')
  })

  // Engine restart
  ipcMain.handle('engine:restart', async () => {
    return restartEngine()
  })

  // ── Zoom factor (Ctrl+/- webContents zoom) ──

  ipcMain.handle('get-zoom-factor', (event) => {
    const wc = BrowserWindow.fromWebContents(event.sender)?.webContents
    return wc?.zoomFactor ?? 1.0
  })

  ipcMain.handle('set-zoom-factor', (event, factor: number) => {
    const wc = BrowserWindow.fromWebContents(event.sender)?.webContents
    if (wc) {
      wc.zoomFactor = factor
      setStoreKey('zoomFactor', factor)
    }
  })

  // ── Git branch operations ──

  ipcMain.handle('git:branches', async (_event, workspaceRoot: string) => {
    return getGitBranches(workspaceRoot)
  })

  ipcMain.handle('git:switch-branch', async (_event, workspaceRoot: string, branch: string) => {
    return switchGitBranch(workspaceRoot, branch)
  })

  ipcMain.handle('git:uncommitted-changes', async (_event, workspaceRoot: string) => { return getUncommittedChanges(workspaceRoot) })
  ipcMain.handle('git:commit', async (_event, workspaceRoot: string, message: string) => { return gitCommit(workspaceRoot, message) })
  ipcMain.handle('git:push', async (_event, workspaceRoot: string) => { return gitPush(workspaceRoot) })
  ipcMain.handle('git:create-and-switch-branch', async (_event, workspaceRoot: string, branch: string) => {
    return createAndSwitchGitBranch(workspaceRoot, branch)
  })

  // Native OS notification
  ipcMain.handle('show-notification', (event, title: string, body: string) => {
    const win = BrowserWindow.fromWebContents(event.sender)
    try {
      if (Notification.isSupported()) {
        const iconPath = app.isPackaged
          ? path.join(process.resourcesPath, 'icon.ico')
          : path.join(__dirname, '../../src/asset/icon_dev.ico')
        const n = new Notification({ title, body, icon: iconPath })
        n.on('click', () => {
          if (win) {
            if (win.isMinimized()) win.restore()
            win.show()
            win.focus()
          }
        })
        n.show()
        console.log('[notification] native notification shown:', title)
        return
      }
    } catch (e) {
      console.warn('[notification] native notification failed:', e)
    }
    // Fallback: flash the window to get user's attention
    console.log('[notification] falling back to flashFrame')
    if (win) {
      win.flashFrame(true)
    }
  })
}
