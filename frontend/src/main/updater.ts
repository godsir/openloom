import { autoUpdater } from 'electron-updater'
import { app, BrowserWindow } from 'electron'
import { getStoreKey, setStoreKey } from './store'

let initialized = false

/** Read the preferred update channel from config.json (preferences section). */
export function getUpdateChannel(): 'stable' | 'beta' {
  return (getStoreKey<string>('update_channel', 'stable') === 'beta') ? 'beta' : 'stable'
}

export function setUpdateChannel(channel: 'stable' | 'beta'): void {
  setStoreKey('update_channel', channel)
  // Reconfigure and re-check immediately so the user sees the new channel's
  // latest release right away.
  configureUpdater()
  autoUpdater.checkForUpdates().catch(() => {})
}

function configureUpdater(): void {
  const channel = getStoreKey<string>('update_channel', 'stable')
  autoUpdater.setFeedURL({
    provider: 'github',
    owner: 'godsir',
    repo: 'openloom',
  })
  const isBeta = channel === 'beta'
  if (isBeta) {
    autoUpdater.channel = 'beta'
  } else {
    // The channel setter validates when transitioning from a non-null
    // value, preventing us from clearing it. Reset internal field directly.
    ;(autoUpdater as any)._channel = null
    autoUpdater.allowDowngrade = false
  }
  autoUpdater.allowPrerelease = isBeta
  autoUpdater.autoDownload = false
  autoUpdater.autoInstallOnAppQuit = false

  // Allow update checks in dev mode too (electron-updater skips unless
  // app.isPackaged or forceDevUpdateConfig is true).
  if (!app.isPackaged) {
    autoUpdater.forceDevUpdateConfig = true
  }
}

export function setupAutoUpdater(mainWindow: BrowserWindow): void {
  if (initialized) return
  initialized = true

  configureUpdater()

  autoUpdater.on('update-available', (info) => {
    mainWindow.webContents.send('update-available', info)
  })

  autoUpdater.on('update-not-available', () => {
    mainWindow.webContents.send('update-not-available')
  })

  autoUpdater.on('download-progress', (progress) => {
    mainWindow.webContents.send('update-download-progress', progress)
  })

  autoUpdater.on('update-downloaded', () => {
    mainWindow.webContents.send('update-downloaded')
  })

  autoUpdater.on('error', (err) => {
    console.error('[updater] error:', err.message)
    mainWindow.webContents.send('update-error', err.message)
  })

  // Background checks: 30s after startup then every 4 hours
  setTimeout(() => {
    autoUpdater.checkForUpdates().catch(() => {})
  }, 30000)

  setInterval(() => {
    autoUpdater.checkForUpdates().catch(() => {})
  }, 4 * 60 * 60 * 1000)
}

export function checkForUpdates(): void {
  autoUpdater.checkForUpdates().catch(() => {})
}

export function downloadUpdate(): void {
  autoUpdater.downloadUpdate().catch(() => {})
}

export function installUpdate(): void {
  autoUpdater.quitAndInstall()
}
