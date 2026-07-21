import { autoUpdater, CancellationToken } from 'electron-updater'
import { app, BrowserWindow, session } from 'electron'
import { getStoreKey, readToolPrefs, setStoreKey } from './store'

let initialized = false
let activeCheck: Promise<void> | null = null
let updateOperation: 'checking' | 'downloading' | null = null
/** Token for the in-flight background download — cancel() aborts it (C20). */
let downloadCancelToken: CancellationToken | null = null
/** Set while a cancel is in progress so the 'error' event isn't shown as a failure. */
let suppressingDownloadError = false
let mainWindowRef: BrowserWindow | null = null
const BETA_UPDATE_URL = 'https://github.com/godsir/openloom/releases/download/beta'

type ProxyPreferences = {
  proxy_enabled?: boolean
  http_proxy?: string
}

/**
 * Keep Chromium's default session and electron-updater's separate session in
 * sync. `proxy-auto-detect` forces expensive WPAD/PAC discovery. Updates
 * always use the OS network path so they still work when API/web proxy use is
 * disabled but the machine relies on a system or transparent global proxy.
 */
export async function configureUpdaterProxy(): Promise<void> {
  const toolPrefs = readToolPrefs() as ProxyPreferences
  const enabled = toolPrefs.proxy_enabled === true
  const customProxy = toolPrefs.http_proxy?.trim()
  const appProxyConfig = !enabled
    ? { mode: 'direct' as const }
    : customProxy
      ? { mode: 'fixed_servers' as const, proxyRules: customProxy }
      : { mode: 'system' as const }
  const updaterProxyConfig = enabled && customProxy
    ? { mode: 'fixed_servers' as const, proxyRules: customProxy }
    : { mode: 'system' as const }

  await Promise.all([
    session.defaultSession.setProxy(appProxyConfig),
    autoUpdater.netSession.setProxy(updaterProxyConfig),
  ])
  console.log(`[proxy] Electron app: ${appProxyConfig.mode}; updater: ${updaterProxyConfig.mode}`)
}

/** Read the preferred update channel from config.json (preferences section). */
export function getUpdateChannel(): 'stable' | 'beta' {
  return (getStoreKey<string>('update_channel', 'stable') === 'beta') ? 'beta' : 'stable'
}

export async function setUpdateChannel(channel: 'stable' | 'beta'): Promise<void> {
  setStoreKey('update_channel', channel)
  // A check already in progress retains its provider configuration. Queue this
  // check after it so a channel change cannot reuse an in-flight stable/beta
  // request and report the wrong channel's result.
  await checkForUpdates()
}

function configureUpdater(): void {
  const channel = getStoreKey<string>('update_channel', 'stable')
  const isBeta = channel === 'beta'
  if (isBeta) {
    // GitHubProvider's prerelease selection can choose the newest stable
    // release from its Atom feed before it reads beta metadata. The dedicated
    // beta release instead exposes beta.yml directly, so it cannot fall back
    // to a stable release during a beta check.
    autoUpdater.setFeedURL({ provider: 'generic', url: BETA_UPDATE_URL })
    autoUpdater.channel = 'beta'
  } else {
    autoUpdater.setFeedURL({
      provider: 'github',
      owner: 'godsir',
      repo: 'openloom',
    })
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
  mainWindowRef = mainWindow

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
    // Network failures while polling for updates are not failed installations.
    // Only surface download errors in the update UI; check failures remain
    // logged and can be retried from Settings. A user-initiated cancel is
    // suppressed here — it's reported via 'update-download-cancelled' instead.
    if (updateOperation === 'downloading' && !suppressingDownloadError) {
      mainWindow.webContents.send('update-error', err.message)
    }
  })

  // Background checks: 30s after startup then every 4 hours
  setTimeout(() => {
    void checkForUpdates().catch(() => {})
  }, 30000)

  setInterval(() => {
    void checkForUpdates().catch(() => {})
  }, 4 * 60 * 60 * 1000)
}

export async function checkForUpdates(): Promise<void> {
  // Keep a reference to the check that was active when this call began. If a
  // channel switch arrives during a check, it waits, then configures and checks
  // again using the newly selected channel.
  const previousCheck = activeCheck
  if (previousCheck) {
    await previousCheck.catch(() => {})
  }

  await configureUpdaterProxy()
  configureUpdater()
  updateOperation = 'checking'
  const check = autoUpdater.checkForUpdates().then(() => {})
  activeCheck = check
  try {
    await check
  } finally {
    if (activeCheck === check) activeCheck = null
    if (updateOperation === 'checking') updateOperation = null
  }
}

export async function downloadUpdate(): Promise<void> {
  await configureUpdaterProxy()
  updateOperation = 'downloading'
  suppressingDownloadError = false
  const token = new CancellationToken()
  downloadCancelToken = token
  try {
    await autoUpdater.downloadUpdate(token)
  } catch (err) {
    // Cancelling aborts the download and rejects the promise. That's expected,
    // not a failure — the renderer already got 'update-download-cancelled'.
    if (token.cancelled) {
      return
    }
    throw err
  } finally {
    if (downloadCancelToken === token) downloadCancelToken = null
    if (updateOperation === 'downloading') updateOperation = null
  }
}

/** Abort an in-flight background download (C20). No-op if nothing is downloading. */
export function cancelDownloadUpdate(): void {
  if (!downloadCancelToken) return
  suppressingDownloadError = true
  downloadCancelToken.cancel()
  downloadCancelToken = null
  if (updateOperation === 'downloading') updateOperation = null
  mainWindowRef?.webContents.send('update-download-cancelled')
}

export function installUpdate(): void {
  autoUpdater.quitAndInstall()
}
