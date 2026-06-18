import { Tray, Menu, BrowserWindow, app, nativeImage } from 'electron'
import { join } from 'path'
import { existsSync } from 'fs'
import { togglePetDnd, getPetDnd, setOnDndChanged, isPetEnabled, togglePet, setOnPetToggled } from './pet'
import { t } from './i18n'

let tray: Tray | null = null
let mainWindow: BrowserWindow | null = null

function getIconPath(): string {
  if (app.isPackaged) {
    return join(process.resourcesPath, 'icon.png')
  }
  return join(__dirname, '../../src/asset/icon_dev.png')
}

function buildMenu(): Menu {
  const petOn = isPetEnabled()
  const dnd = getPetDnd()
  return Menu.buildFromTemplate([
    {
      label: t('tray.showLoom'),
      click: () => {
        mainWindow?.show()
        mainWindow?.focus()
      },
    },
    { type: 'separator' },
    {
      label: petOn ? t('tray.hidePet') : t('tray.showPet'),
      click: () => { togglePet() },
    },
    {
      label: dnd ? t('tray.dndOff') : t('tray.dndOn'),
      enabled: petOn,
      click: () => { togglePetDnd() },
    },
    { type: 'separator' },
    {
      label: t('tray.settings'),
      click: () => {
        mainWindow?.show()
        mainWindow?.focus()
        mainWindow?.webContents.send('navigate', '/settings')
      },
    },
    { type: 'separator' },
    {
      label: t('tray.quit'),
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
  setOnPetToggled(refreshMenu)
}
