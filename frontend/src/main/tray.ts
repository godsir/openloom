import { Tray, Menu, BrowserWindow, app, nativeImage } from 'electron'
import { join } from 'path'
import { existsSync } from 'fs'
import { togglePetDnd, getPetDnd, setOnDndChanged } from './pet'

let tray: Tray | null = null
let mainWindow: BrowserWindow | null = null

function getIconPath(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'icon.png')
  }
  return join(__dirname, '../../src/asset/loom_logo_dev.png')
}

function buildMenu(): Menu {
  const dnd = getPetDnd()
  return Menu.buildFromTemplate([
    {
      label: '显示 openLoom',
      click: () => {
        mainWindow?.show()
        mainWindow?.focus()
      },
    },
    {
      label: dnd ? '关闭勿扰模式' : '开启勿扰模式',
      click: () => { togglePetDnd() },
    },
    { type: 'separator' },
    {
      label: '退出',
      click: () => {
        app.quit()
      },
    },
  ])
}

function refreshMenu(): void {
  if (tray) tray.setContextMenu(buildMenu())
}

export function createTray(mw: BrowserWindow): void {
  mainWindow = mw
  const iconPath = getIconPath()
  const icon = existsSync(iconPath)
    ? nativeImage.createFromPath(iconPath)
    : nativeImage.createEmpty()

  tray = new Tray(icon.resize({ width: 16, height: 16 }))

  tray.setToolTip('openLoom')
  tray.setContextMenu(buildMenu())

  tray.on('double-click', () => {
    mainWindow?.show()
    mainWindow?.focus()
  })

  setOnDndChanged(refreshMenu)
}
