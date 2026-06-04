import { app, BrowserWindow, protocol } from 'electron'
import { createMainWindow, getMainWindow } from './window'
import { registerIpcHandlers } from './ipc'
import { startEngine, stopEngine } from './engine'
import { createTray } from './tray'
import { setupAutoUpdater, checkForUpdates } from './updater'
import { getStoreKey } from './store'
import { initPet, registerPetProtocol } from './pet'
import { startConfigWatcher } from './config-watcher'

// Windows tuning.
if (process.platform === 'win32') {
  // Keep timers running so streaming flushes stay smooth when the window
  // is in the background or partially occluded.
  app.commandLine.appendSwitch('disable-background-timer-throttling')

  // User escape hatch — only honoured if explicitly enabled via Settings.
  // Falls back to software compositing; reduces animation smoothness but
  // can work around driver/compositor flicker on some Win11 setups.
  const disableHwAccel = getStoreKey<boolean>('disableHardwareAcceleration', false)
  if (disableHwAccel) {
    app.disableHardwareAcceleration()
  }
}

let port = 0
let isQuitting = false

protocol.registerSchemesAsPrivileged([
  { scheme: 'loom-pet', privileges: { standard: true, secure: true, supportFetchAPI: true } },
])

// Single instance lock MUST be requested before the ready event.
// Calling it inside whenReady() is too late and causes a native
// Electron window to flash on auto-start.
if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', () => {
    const win = getMainWindow()
    if (win) {
      if (win.isMinimized()) win.restore()
      win.show()
      win.focus()
    }
  })
}

app.whenReady().then(async () => {
  registerPetProtocol()

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
  try {
    initPet() // Desktop pet
  } catch (e) {
    console.error('[pet] initPet failed:', e)
  }

  // Start 30-second config directory poll watcher for model config hot-reload
  startConfigWatcher()

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
