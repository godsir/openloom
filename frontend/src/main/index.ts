import { app, BrowserWindow } from 'electron'
import { createMainWindow, getMainWindow } from './window'
import { registerIpcHandlers } from './ipc'
import { startEngine, stopEngine } from './engine'
import { createTray } from './tray'
import { setupAutoUpdater, checkForUpdates } from './updater'

let port = 0

app.whenReady().then(async () => {
  if (!app.requestSingleInstanceLock()) {
    app.quit()
    return
  }

  app.on('second-instance', () => {
    const win = getMainWindow()
    if (win) {
      if (win.isMinimized()) win.restore()
      win.focus()
    }
  })

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
    }
  })
})

app.on('window-all-closed', async () => {
  await stopEngine()
  app.quit()
})
