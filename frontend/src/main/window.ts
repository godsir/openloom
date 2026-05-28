import { BrowserWindow, screen, app } from 'electron'
import { join } from 'path'

let mainWindow: BrowserWindow | null = null

function getIconPath(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'icon.ico')
  }
  return join(__dirname, '../../src/asset/loom_logo_dev.ico')
}

export function createMainWindow(port: number): BrowserWindow {
  const { width: screenWidth, height: screenHeight } = screen.getPrimaryDisplay().workAreaSize
  const width = Math.min(1200, Math.floor(screenWidth * 0.75))
  const height = Math.min(800, Math.floor(screenHeight * 0.85))

  mainWindow = new BrowserWindow({
    width,
    height,
    minWidth: 680,
    minHeight: 400,
    frame: false,
    titleBarStyle: 'hidden',
    backgroundColor: '#1a1a2e',
    show: false,
    icon: getIconPath(),
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      preload: join(__dirname, '../preload/index.js'),
    },
  })

  mainWindow.on('ready-to-show', () => {
    mainWindow?.show()
  })

  mainWindow.on('closed', () => {
    mainWindow = null
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
