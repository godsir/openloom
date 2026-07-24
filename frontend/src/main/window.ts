import { BrowserWindow, screen, app, shell, ipcMain } from 'electron'
import { join } from 'path'
import { getStoreKey } from './store'
import { isSafeExternalWindowUrl, isWriteAssistantWindowRequest } from './write-assistant-window'

let mainWindow: BrowserWindow | null = null
let writeAssistantWindow: BrowserWindow | null = null

function getIconPath(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'icon.ico')
  }
  return join(__dirname, '../../src/asset/icon_dev.ico')
}

export function createMainWindow(port: number): BrowserWindow {
  const { width: screenWidth, height: screenHeight } = screen.getPrimaryDisplay().workAreaSize
  const width = Math.min(1440, Math.floor(screenWidth * 0.82))
  const height = Math.min(900, Math.floor(screenHeight * 0.88))

  // 静默启动：开机自启 + startToTray 时窗口不自动弹出
  const startSilent = process.argv.includes('--start-hidden') && getStoreKey('startToTray', false)

  mainWindow = new BrowserWindow({
    width,
    height,
    minWidth: 680,
    minHeight: 400,
    frame: false,
    titleBarStyle: 'hidden',
    transparent: true,
    backgroundColor: '#00000000',
    show: false,
    icon: getIconPath(),
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      backgroundThrottling: false,
      preload: join(__dirname, '../preload/index.js'),
    },
  })

  mainWindow.on('ready-to-show', () => {
    if (!startSilent) mainWindow?.show()
  })

  // Restore saved zoom factor (default 1.0), overriding any stale Chromium zoom
  const savedZoom = getStoreKey('zoomFactor', 1.0) as number
  mainWindow.webContents.on('did-finish-load', () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.zoomFactor = savedZoom
    }
  })

  mainWindow.on('closed', () => {
    mainWindow = null
  })

  // Prevent any link click from hijacking the app window
  mainWindow.webContents.setWindowOpenHandler(({ url, frameName }) => {
    if (isWriteAssistantWindowRequest(url, frameName)) {
      return {
        action: 'allow',
        overrideBrowserWindowOptions: {
          width: 380,
          height: 640,
          minWidth: 320,
          minHeight: 400,
          frame: false,
          resizable: true,
          minimizable: true,
          maximizable: true,
          fullscreenable: false,
          autoHideMenuBar: true,
          backgroundColor: '#171717',
          title: 'AI Writing Assistant',
          icon: getIconPath(),
          webPreferences: {
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: true,
          },
        },
      }
    }
    shell.openExternal(url)
    return { action: 'deny' }
  })
  mainWindow.webContents.on('did-create-window', (childWindow, details) => {
    if (!isWriteAssistantWindowRequest(details.url, details.frameName)) return

    writeAssistantWindow = childWindow
    childWindow.on('closed', () => {
      if (writeAssistantWindow === childWindow) writeAssistantWindow = null
    })
    childWindow.webContents.setWindowOpenHandler(({ url }) => {
      if (isSafeExternalWindowUrl(url)) void shell.openExternal(url)
      return { action: 'deny' }
    })
    childWindow.webContents.on('will-navigate', (event, url) => {
      event.preventDefault()
      if (isSafeExternalWindowUrl(url)) void shell.openExternal(url)
    })
  })
  mainWindow.webContents.on('will-navigate', (event, url) => {
    // Allow only the initial load; block all other navigations
    const isInitialLoad = mainWindow?.webContents.getURL() === '' || mainWindow?.webContents.getURL() === 'about:blank'
    if (!isInitialLoad) {
      event.preventDefault()
      shell.openExternal(url)
    }
  })

  // Forward context-menu events to the renderer so it can display a
  // theme-aware custom HTML menu instead of the OS-native popup.
  mainWindow.webContents.on('context-menu', (_event, params) => {
    _event.preventDefault()
    mainWindow?.webContents.send('context-menu', {
      isEditable: params.isEditable,
      canCut: params.editFlags.canCut,
      canCopy: params.editFlags.canCopy,
      canPaste: params.editFlags.canPaste,
      canSelectAll: params.editFlags.canSelectAll,
      hasSelection: !!(params.selectionText && params.selectionText.trim().length > 0),
      x: params.x,
      y: params.y,
    })
  })

  // Execute cut/copy/paste/selectAll on behalf of the custom context menu.
  ipcMain.on('context-menu-action', (_event, action: string) => {
    const wc = mainWindow?.webContents
    if (!wc) return
    switch (action) {
      case 'cut': wc.cut(); break
      case 'copy': wc.copy(); break
      case 'paste': wc.paste(); break
      case 'selectAll': wc.selectAll(); break
    }
  })

  // Inject port and isPackaged AFTER page loads (dev mode loadURL resets pre-load context)
  mainWindow.webContents.on('did-finish-load', () => {
    mainWindow?.webContents.executeJavaScript(`window.__enginePort__ = ${port}; window.__isPackaged__ = ${app.isPackaged}; console.log('[main] port injected:', ${port})`)
  })

  if (!app.isPackaged) {
    mainWindow.loadURL('http://localhost:5173')
    mainWindow.webContents.openDevTools({ mode: 'detach' })
  } else {
    mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }

  return mainWindow
}

export function getMainWindow(): BrowserWindow | null {
  return mainWindow
}

export function minimizeWriteAssistantWindow(): void {
  writeAssistantWindow?.minimize()
}

export function maximizeWriteAssistantWindow(): void {
  if (writeAssistantWindow?.isMaximized()) {
    writeAssistantWindow.unmaximize()
  } else {
    writeAssistantWindow?.maximize()
  }
}

export function closeWriteAssistantWindow(): void {
  writeAssistantWindow?.close()
}
