import { Tray, Menu, BrowserWindow, app, nativeImage } from 'electron'
import { join } from 'path'
import { existsSync } from 'fs'

let tray: Tray | null = null

function getIconPath(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'icon.png')
  }
  return join(__dirname, '../../src/asset/loom_logo_dev.png')
}

export function createTray(mainWindow: BrowserWindow): void {
  const iconPath = getIconPath()
  const icon = existsSync(iconPath)
    ? nativeImage.createFromPath(iconPath)
    : nativeImage.createEmpty()

  tray = new Tray(icon.resize({ width: 16, height: 16 }))

  const contextMenu = Menu.buildFromTemplate([
    {
      label: '显示 openLoom',
      click: () => {
        mainWindow.show()
        mainWindow.focus()
      },
    },
    { type: 'separator' },
    {
      label: '退出',
      click: () => {
        app.quit()
      },
    },
  ])

  tray.setToolTip('openLoom')
  tray.setContextMenu(contextMenu)

  tray.on('double-click', () => {
    mainWindow.show()
    mainWindow.focus()
  })
}
