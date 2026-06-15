import { BrowserWindow, screen, app, shell, Menu } from 'electron'
import { join } from 'path'
import { t } from './i18n'

let mainWindow: BrowserWindow | null = null

function getIconPath(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'icon.ico')
  }
  return join(__dirname, '../../src/asset/loom_logo_dev.ico')
}

export function createMainWindow(port: number): BrowserWindow {
  const { width: screenWidth, height: screenHeight } = screen.getPrimaryDisplay().workAreaSize
  const width = Math.min(1440, Math.floor(screenWidth * 0.82))
  const height = Math.min(900, Math.floor(screenHeight * 0.88))

  mainWindow = new BrowserWindow({
    width,
    height,
    minWidth: 680,
    minHeight: 400,
    frame: false,
    titleBarStyle: 'hidden',
    backgroundColor: '#0B0F14',
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
    mainWindow?.show()
  })

  mainWindow.on('closed', () => {
    mainWindow = null
  })

  // Prevent any link click from hijacking the app window
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url)
    return { action: 'deny' }
  })
  mainWindow.webContents.on('will-navigate', (event, url) => {
    // Allow only the initial load; block all other navigations
    const isInitialLoad = mainWindow?.webContents.getURL() === '' || mainWindow?.webContents.getURL() === 'about:blank'
    if (!isInitialLoad) {
      event.preventDefault()
      shell.openExternal(url)
    }
  })

  // Native context menu for text-editing fields — labels follow the app's locale.
  // `role` handles the action; `accelerator` is set explicitly because Electron
  // popup menus do not auto-render the shortcut from role alone.
  mainWindow.webContents.on('context-menu', (_event, params) => {
    const template: Electron.MenuItemConstructorOptions[] = []

    if (params.isEditable) {
      template.push(
        { label: t('menu.cut'),       role: 'cut',       accelerator: 'CmdOrCtrl+X', enabled: params.editFlags.canCut },
        { label: t('menu.copy'),      role: 'copy',      accelerator: 'CmdOrCtrl+C', enabled: params.editFlags.canCopy },
        { label: t('menu.paste'),     role: 'paste',     accelerator: 'CmdOrCtrl+V', enabled: params.editFlags.canPaste },
        { type: 'separator' },
        { label: t('menu.selectAll'), role: 'selectAll', accelerator: 'CmdOrCtrl+A', enabled: params.editFlags.canSelectAll },
      )
    } else if (params.selectionText && params.selectionText.trim().length > 0) {
      template.push({ label: t('menu.copy'), role: 'copy', accelerator: 'CmdOrCtrl+C' })
    }

    if (template.length > 0) {
      const menu = Menu.buildFromTemplate(template)
      menu.popup({ window: mainWindow! })
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
