import { app, BrowserWindow } from 'electron'
import { createMainWindow, getMainWindow } from './window'
import { registerIpcHandlers } from './ipc'
import { startEngine, stopEngine } from './engine'
import { createTray } from './tray'
import { setupAutoUpdater, checkForUpdates } from './updater'
import { getStoreKey, readStore } from './store'

let port = 0
let isQuitting = false

app.whenReady().then(async () => {
  if (!app.requestSingleInstanceLock()) {
    app.quit()
    return
  }

  app.on('second-instance', () => {
    const win = getMainWindow()
    if (win) {
      if (win.isMinimized()) win.restore()
      win.show()
      win.focus()
    }
  })

  // Auto-start on boot
  const autoStart = getStoreKey('autoStart', false)
  app.setLoginItemSettings({ openAtLogin: autoStart })

  registerIpcHandlers()

  try {
    port = await startEngine()
  } catch (e) {
    console.error('Failed to start engine:', e)
    app.quit()
    return
  }

  const win = createMainWindow(port)
  createTray(win)

  // Auto-updater (only in production)
  if (app.isPackaged) {
    setupAutoUpdater(win)
    checkForUpdates()
  }

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow(port)
    } else {
      getMainWindow()?.show()
    }
  })
})

app.on('before-quit', () => {
  isQuitting = true
})

app.on('window-all-closed', async () => {
  if (isQuitting) {
    await stopEngine()
    app.quit()
  }
  // Otherwise: close-to-tray is active, just keep running in tray
})
