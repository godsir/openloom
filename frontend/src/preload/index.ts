import { contextBridge, ipcRenderer } from 'electron'

export interface LoomApi {
  getPlatform: () => Promise<string>
  getAppVersion: () => Promise<string>
  selectFolder: () => Promise<string | null>
  selectFiles: (options?: { filters?: { name: string; extensions: string[] }[] }) => Promise<string[]>
  readFile: (filePath: string, options?: { startLine?: number; endLine?: number }) => Promise<string | null>
  openExternal: (url: string) => Promise<void>
  openFolder: (filePath: string) => void
  openFile: (filePath: string) => Promise<void>
  windowMinimize: () => void
  windowMaximize: () => void
  windowClose: () => void
  windowIsMaximized: () => Promise<boolean>
  getPreference: <T>(key: string, fallback: T) => Promise<T>
  setPreference: (key: string, value: unknown) => Promise<void>
  checkForUpdates: () => Promise<void>
  downloadUpdate: () => Promise<void>
  installUpdate: () => void
  onUpdateAvailable: (cb: (info: unknown) => void) => () => void
  onUpdateNotAvailable: (cb: () => void) => () => void
  onUpdateDownloadProgress: (cb: (progress: { percent: number; bytesPerSecond: number; transferred: number; total: number }) => void) => () => void
  onUpdateDownloaded: (cb: () => void) => () => void
  onUpdateError: (cb: (msg: string) => void) => () => void
  getLoomDir: () => Promise<string>
  togglePet: (on: boolean) => Promise<boolean>
  resizePet: (spriteSize: number) => void
  listPets: () => Promise<PetMeta[]>
  restartEngine: () => Promise<number>
  onEngineStateChanged: (cb: (payload: { state: string; port: number | null }) => void) => () => void
  /** Model config files changed on disk — renderer should refresh via model.list. */
  onModelConfigChanged: (cb: () => void) => () => void
  /** Navigate to a route (triggered from tray menu). */
  onNavigate: (cb: (route: string) => void) => () => void
  /** Workspace file write methods */
  readWorkspaceImage: (filePath: string, workspaceRoot: string) => Promise<{ok: boolean; dataUrl?: string; mimeType?: string; message?: string}>
  readWorkspaceBinary: (filePath: string, workspaceRoot: string) => Promise<{ok: boolean; data?: string; size?: number; message?: string}>
  exportWriteHtml: (html: string, title: string) => Promise<{ok: boolean; path?: string; error?: string}>
  exportWritePdf: (html: string, title: string) => Promise<{ok: boolean; path?: string; error?: string}>
  exportWriteDocx: (html: string, title: string) => Promise<{ok: boolean; path?: string; error?: string}>
  copyWriteDocumentAsRichText: (filePath: string, workspaceRoot: string, content: string) => Promise<{ok: boolean; message?: string}>
  watchFile: (filePath: string, workspaceRoot: string) => Promise<{ok: boolean}>
  unwatchFile: (filePath: string, workspaceRoot: string) => Promise<{ok: boolean}>
  /** Show a native OS notification (Windows toast / macOS notification center) */
  showNotification: (title: string, body: string) => Promise<void>
}

interface PetMeta {
  id: string
  displayName: string
  description: string
  spritesheetPath: string
  frameWidth?: number
  frameHeight?: number
  columns?: number
  rows?: number
  framesPerRow?: number
  rowFrames?: Record<string, number>
  states?: Record<string, number>
}

contextBridge.exposeInMainWorld('loom', {
  getPlatform: () => ipcRenderer.invoke('get-platform'),
  getAppVersion: () => ipcRenderer.invoke('get-app-version'),
  selectFolder: () => ipcRenderer.invoke('select-folder'),
  selectFiles: (options?: { filters?: { name: string; extensions: string[] }[] }) =>
    ipcRenderer.invoke('select-files', options),
  readFile: (filePath: string, options?: { startLine?: number; endLine?: number }) =>
    ipcRenderer.invoke('read-file', filePath, options),
  openExternal: (url: string) => ipcRenderer.invoke('open-external', url),
  openFolder: (filePath: string) => { ipcRenderer.invoke('open-folder', filePath) },
  openFile: (filePath: string) => ipcRenderer.invoke('open-file', filePath),
  windowMinimize: () => { ipcRenderer.invoke('window-minimize') },
  windowMaximize: () => { ipcRenderer.invoke('window-maximize') },
  windowClose: () => { ipcRenderer.invoke('window-close') },
  windowIsMaximized: () => ipcRenderer.invoke('window-is-maximized'),
  getPreference: <T>(key: string, fallback: T) => ipcRenderer.invoke('get-preference', key, fallback),
  setPreference: (key: string, value: unknown) => ipcRenderer.invoke('set-preference', key, value),
  checkForUpdates: () => ipcRenderer.invoke('check-for-updates'),
  downloadUpdate: () => ipcRenderer.invoke('download-update'),
  installUpdate: () => ipcRenderer.invoke('install-update'),
  onUpdateAvailable: (cb: (info: unknown) => void) => {
    const fn = (_e: unknown, info: unknown): void => cb(info)
    ipcRenderer.on('update-available', fn)
    return () => { ipcRenderer.removeListener('update-available', fn) }
  },
  onUpdateNotAvailable: (cb: () => void) => {
    const fn = (): void => cb()
    ipcRenderer.on('update-not-available', fn)
    return () => { ipcRenderer.removeListener('update-not-available', fn) }
  },
  onUpdateDownloadProgress: (cb: (progress: { percent: number; bytesPerSecond: number; transferred: number; total: number }) => void) => {
    const fn = (_e: unknown, progress: { percent: number; bytesPerSecond: number; transferred: number; total: number }): void => cb(progress)
    ipcRenderer.on('update-download-progress', fn)
    return () => { ipcRenderer.removeListener('update-download-progress', fn) }
  },
  onUpdateDownloaded: (cb: () => void) => {
    const fn = (): void => cb()
    ipcRenderer.on('update-downloaded', fn)
    return () => { ipcRenderer.removeListener('update-downloaded', fn) }
  },
  onUpdateError: (cb: (msg: string) => void) => {
    const fn = (_e: unknown, msg: string): void => cb(msg)
    ipcRenderer.on('update-error', fn)
    return () => { ipcRenderer.removeListener('update-error', fn) }
  },
  getLoomDir: () => ipcRenderer.invoke('get-loom-dir'),
  togglePet: (on: boolean) => ipcRenderer.invoke('pet:toggle', on),
  resizePet: (spriteSize: number) => ipcRenderer.send('pet:resize', spriteSize),
  listPets: () => ipcRenderer.invoke('pets:list'),
  restartEngine: () => ipcRenderer.invoke('engine:restart'),
  onEngineStateChanged: (cb: (payload: { state: string; port: number | null }) => void) => {
    const fn = (_e: unknown, payload: { state: string; port: number | null }): void => cb(payload)
    ipcRenderer.on('engine:state-changed', fn)
    return () => { ipcRenderer.removeListener('engine:state-changed', fn) }
  },
  onModelConfigChanged: (cb: () => void) => {
    const fn = (): void => cb()
    ipcRenderer.on('model-config-changed', fn)
    return () => { ipcRenderer.removeListener('model-config-changed', fn) }
  },
  /** Navigate to a route (triggered from tray menu). */
  onNavigate: (cb: (route: string) => void) => {
    const fn = (_e: unknown, route: string): void => cb(route)
    ipcRenderer.on('navigate', fn)
    return () => { ipcRenderer.removeListener('navigate', fn) }
  },
  readWorkspaceImage: (filePath: string, workspaceRoot: string) => ipcRenderer.invoke('write:read-image', { filePath, workspaceRoot }),
  readWorkspaceBinary: (filePath: string, workspaceRoot: string) => ipcRenderer.invoke('write:read-binary', { filePath, workspaceRoot }),
  exportWriteHtml: (html: string, title: string) => ipcRenderer.invoke('write:export-html-enhanced', html, title),
  exportWritePdf: (html: string, title: string) => ipcRenderer.invoke('write:export-pdf', html, title),
  exportWriteDocx: (html: string, title: string) => ipcRenderer.invoke('write:export-docx', html, title),
  copyWriteDocumentAsRichText: (filePath: string, workspaceRoot: string, content: string) => ipcRenderer.invoke('write:copy-rich-text', { filePath, workspaceRoot, content }),
  watchFile: (filePath: string, workspaceRoot: string) => ipcRenderer.invoke('write:watch-file', { filePath, workspaceRoot }),
  unwatchFile: (filePath: string, workspaceRoot: string) => ipcRenderer.invoke('write:unwatch-file', { filePath, workspaceRoot }),
  showNotification: (title: string, body: string) => ipcRenderer.invoke('show-notification', title, body),
} satisfies LoomApi)
