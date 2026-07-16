import { app, BrowserWindow, protocol } from 'electron'
import { createMainWindow, getMainWindow } from './window'
import { registerIpcHandlers } from './ipc'
import { startEngine, stopEngine } from './engine'
import { createTray } from './tray'
import { setupAutoUpdater, checkForUpdates, configureUpdaterProxy } from './updater'
import { getStoreKey } from './store'
import { initPet, registerPetProtocol } from './pet'
import { startConfigWatcher } from './config-watcher'
import { join } from 'path'
import { homedir } from 'os'
import { existsSync, mkdirSync } from 'fs'
import Database from 'better-sqlite3'
import { IMStore, IMGatewayManager } from './im'
import { ImBridge } from './im/imBridge'

// Windows tuning.
if (process.platform === 'win32') {
  // Required for native Windows toast notifications (Notification API).
  // Must match the electron-builder appId so toast grouping and update
  // identity stay consistent with the installed app.
  app.setAppUserModelId('com.openloom.app')

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
//
// Skip the lock in dev mode so that `npm run dev` can run alongside
// a packaged production release without clashing.
if (!app.isPackaged || app.requestSingleInstanceLock()) {
  if (app.isPackaged) {
    app.on('second-instance', () => {
      const win = getMainWindow()
      if (win) {
        if (win.isMinimized()) win.restore()
        win.show()
        win.focus()
      }
    })
  }
} else {
  app.quit()
}

app.whenReady().then(async () => {
  registerPetProtocol()
  try {
    await configureUpdaterProxy()
  } catch (e) {
    console.error('[proxy] Failed to configure Electron proxy:', e)
  }

  // Auto-start on boot — only apply in packaged/production builds.
  // In dev mode, force-disable to prevent polluting the user's OS startup items.
  if (app.isPackaged) {
    const autoStart = getStoreKey('autoStart', false)
    app.setLoginItemSettings({ openAtLogin: autoStart, args: ['--start-hidden'] })
  } else {
    app.setLoginItemSettings({ openAtLogin: false })
  }

  // IM — instant messaging integration (must init BEFORE registerIpcHandlers
  // so IPC handlers can access __imStore / __imGatewayManager)
  let imGatewayManager: IMGatewayManager | undefined
  try {
    const loomDir = join(homedir(), '.loom')
    if (!existsSync(loomDir)) mkdirSync(loomDir, { recursive: true })
    const imDb = new Database(join(loomDir, 'im.db'))
    imDb.pragma('journal_mode = WAL')
    imDb.pragma('busy_timeout = 3000')

    const imStore = new IMStore(imDb)
    imGatewayManager = new IMGatewayManager({
      imStore,
      onMessage: (msg) => {
        // Forward IM message to renderer via IPC
        getMainWindow()?.webContents.send('im:message', msg)
      },
    })

    // Make available globally for IPC handlers
    ;(global as any).__imStore = imStore
    ;(global as any).__imGatewayManager = imGatewayManager
    ;(global as any).__imDb = imDb

    console.log('[IM] IM gateway manager initialized')
  } catch (e) {
    console.error('[IM] Failed to initialize IM:', e)
  }

  registerIpcHandlers()

  try {
    port = await startEngine()
  } catch (e) {
    console.error('Failed to start engine:', e)
    app.quit()
    return
  }

  // Connect IM bridge to the engine so IM messages reach the agent and replies
  // are sent back through the IM channel.
  const _imStore = (global as any).__imStore as IMStore | undefined
  const _mgr = (global as any).__imGatewayManager as IMGatewayManager | undefined
  if (_imStore && _mgr && port) {
    try {
      const imBridge = new ImBridge(_imStore, () => {
        getMainWindow()?.webContents.send('im:session-changed')
      }, (method, params) => {
        getMainWindow()?.webContents.send('im:stream-event', { method, params })
      })
      imBridge.connect(port)
      _mgr.setBridge(imBridge)
      ;(global as any).__imBridge = imBridge
      console.log('[IM] bridge connected to engine on port', port)
    } catch (e: any) {
      console.error('[IM] bridge init failed:', e?.message ?? e)
    }
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

  // Start all enabled IM channels (needs mainWindow to exist for event forwarding)
  if (imGatewayManager) {
    imGatewayManager.startAllEnabled().catch(err => {
      console.error('[IM] Failed to start IM gateways:', err)
    })
  }

  // Auto-updater
  setupAutoUpdater(win)
  void checkForUpdates().catch(() => {})

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
  // Stop IM gateways
  try {
    const mgr = (global as any).__imGatewayManager as IMGatewayManager | undefined
    if (mgr) mgr.stopAll()
    const bridge = (global as any).__imBridge as ImBridge | undefined
    if (bridge) bridge.disconnect()
    const db = (global as any).__imDb as Database.Database | undefined
    if (db) db.close()
  } catch (e) {
    console.error('[IM] cleanup error:', e)
  }
})

app.on('window-all-closed', async () => {
  if (isQuitting) {
    await stopEngine()
    app.quit()
  }
  // Otherwise: close-to-tray is active, just keep running in tray
})
