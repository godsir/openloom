import { BrowserWindow, screen, ipcMain, protocol, app, Menu } from 'electron'
import { join, extname } from 'path'
import { homedir } from 'os'
import { existsSync, mkdirSync, readdirSync, readFileSync, copyFileSync } from 'fs'
import { getStoreKey, setStoreKey } from './store'

const SIZE_VALUES: Record<string, number> = { small: 128, medium: 192, large: 256 }
const PADDING = 48 // extra space for bubble above
const PETS_DIR = join(homedir(), '.loom', 'pets')
const MIME: Record<string, string> = { '.webp': 'image/webp', '.png': 'image/png', '.jpg': 'image/jpeg', '.gif': 'image/gif' }

let petWindow: BrowserWindow | null = null
let petEnabled = false

export function registerPetProtocol(): void {
  protocol.handle('loom-pet', (request) => {
    const u = new URL(request.url)
    const filePath = join(PETS_DIR, u.hostname + u.pathname)
    try {
      const data = readFileSync(filePath)
      const mime = MIME[extname(filePath).toLowerCase()] || 'image/webp'
      return new Response(data, { headers: { 'content-type': mime, 'cache-control': 'no-cache' } })
    } catch {
      return new Response(null, { status: 404 })
    }
  })
}

function ensurePetsDir(): void {
  if (!existsSync(PETS_DIR)) {
    mkdirSync(PETS_DIR, { recursive: true })
  }
}

function seedDefaultPet(): void {
  const dest = join(PETS_DIR, 'homelander-2')
  if (existsSync(dest)) return
  // Try multiple source paths: built output, source tree, cwd
  const candidates = [
    join(__dirname, '../renderer/pets/homelander-2'),
    join(__dirname, '../../src/renderer/public/pets/homelander-2'),
    join(process.cwd(), 'src/renderer/public/pets/homelander-2'),
    join(app.getAppPath(), 'src/renderer/public/pets/homelander-2'),
  ]
  const src = candidates.find(d => existsSync(join(d, 'pet.json')))
  if (!src) {
    console.error('[pet] seedDefaultPet: source not found, tried:', candidates)
    return
  }
  mkdirSync(dest, { recursive: true })
  copyFileSync(join(src, 'pet.json'), join(dest, 'pet.json'))
  copyFileSync(join(src, 'spritesheet.webp'), join(dest, 'spritesheet.webp'))
}

// IPC: list all installed pets
ipcMain.handle('pets:list', () => {
  ensurePetsDir()
  if (!existsSync(PETS_DIR)) return []
  try {
    return readdirSync(PETS_DIR, { withFileTypes: true })
      .filter(d => d.isDirectory())
      .map(d => {
        const metaPath = join(PETS_DIR, d.name, 'pet.json')
        if (!existsSync(metaPath)) return null
        try {
          const meta = JSON.parse(readFileSync(metaPath, 'utf-8'))
          meta.id = d.name // overwrite id with directory name
          return meta
        } catch { return null }
      })
      .filter(Boolean)
  } catch { return [] }
})

export function initPet(): void {
  ensurePetsDir()
  seedDefaultPet()
  petEnabled = getStoreKey('petEnabled', false) as boolean
  if (petEnabled) create()
}

function sendPetCommand(cmd: string): void {
  if (petWindow && !petWindow.isDestroyed()) {
    petWindow.webContents.send('pet:command', cmd)
  }
}

ipcMain.on('pet:move', (_e, dx: number, dy: number) => {
  if (petWindow && !petWindow.isDestroyed()) {
    const [x, y] = petWindow.getPosition()
    petWindow.setPosition(x + dx, y + dy)
    setStoreKey(posKey(), { x: x + dx, y: y + dy })
  }
})

ipcMain.on('pet:resize', (_e, spriteSize: number) => {
  if (petWindow && !petWindow.isDestroyed()) {
    const newSize = spriteSize + PADDING
    petWindow.setSize(newSize, newSize)
  }
})

ipcMain.on('pet:context-menu', () => {
  const menu = Menu.buildFromTemplate([
    { label: '大小：小 (128px)', click: () => sendPetCommand('size:small') },
    { label: '大小：中 (192px)', click: () => sendPetCommand('size:medium') },
    { label: '大小：大 (256px)', click: () => sendPetCommand('size:large') },
    { type: 'separator' },
    { label: '关闭桌宠', click: () => sendPetCommand('close') },
  ])
  menu.popup({ window: petWindow! })
})

ipcMain.handle('pet:toggle', (_e, on: boolean) => {
  petEnabled = on
  setStoreKey('petEnabled', on)
  if (on) create(); else close()
  return on
})

function posKey(): string { return 'petPosition' }

function getSavedPos(): { x: number; y: number } | null {
  const saved = getStoreKey(posKey(), null) as { x: number; y: number } | null
  if (!saved || typeof saved.x !== 'number' || typeof saved.y !== 'number') return null
  const { width: sw, height: sh } = screen.getPrimaryDisplay().workAreaSize
  if (saved.x < -200 + 40 || saved.x > sw - 40 || saved.y < -200 + 40 || saved.y > sh - 40) return null
  return saved
}

function create(): void {
  if (petWindow?.isDestroyed() === false) return
  const d = screen.getPrimaryDisplay()
  const { width: sw, height: sh } = d.workAreaSize
  const petSize = (getStoreKey('petSize', 'small') as string) || 'small'
  const spriteSize = SIZE_VALUES[petSize] || 128
  const winSize = spriteSize + PADDING
  const saved = getSavedPos()
  const x = saved ? saved.x : sw - winSize - 20
  const y = saved ? saved.y : sh - winSize - 80

  petWindow = new BrowserWindow({
    width: winSize, height: winSize,
    x, y,
    transparent: true, frame: false, resizable: false,
    hasShadow: false, skipTaskbar: true, alwaysOnTop: true,
    focusable: false, backgroundColor: '#00000000',
    webPreferences: {
      contextIsolation: true, nodeIntegration: false, sandbox: false,
      preload: join(__dirname, '../preload/pet.js'),
    },
  })
  // Mouse transparent everywhere — canvas element handles its own events
  petWindow.setIgnoreMouseEvents(true, { forward: true })
  const activePetId = getStoreKey('activePetId', 'homelander-2') as string
  const hash = `${activePetId}&${spriteSize}`
  petWindow.setAlwaysOnTop(true, 'normal')
  if (process.env.ELECTRON_RENDERER_URL) {
    petWindow.loadURL(`${process.env.ELECTRON_RENDERER_URL}/pet.html#${hash}`)
  } else {
    petWindow.loadFile(join(__dirname, '../renderer/pet.html'), { hash })
  }
  petWindow.show()
  petWindow.on('moved', () => {
    if (petWindow && !petWindow.isDestroyed()) {
      const [x, y] = petWindow.getPosition()
      setStoreKey(posKey(), { x, y })
    }
  })
  petWindow.on('closed', () => { petWindow = null })
}

function close(): void {
  petWindow?.close()
}
