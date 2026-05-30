import { autoUpdater } from 'electron-updater'
import { app, BrowserWindow } from 'electron'

let initialized = false

export function setupAutoUpdater(mainWindow: BrowserWindow): void {
  if (initialized) return
  initialized = true

  autoUpdater.setFeedURL({
    provider: 'github',
    owner: 'godsir',
    repo: 'openloom',
  })

  autoUpdater.autoDownload = false
  autoUpdater.autoInstallOnAppQuit = false

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

  // Automatic background checks only in packaged builds
  if (!app.isPackaged) return

  setInterval(() => {
    autoUpdater.checkForUpdates().catch(() => {})
  }, 4 * 60 * 60 * 1000)

  setTimeout(() => {
    autoUpdater.checkForUpdates().catch(() => {})
  }, 30000)
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
