import { contextBridge, ipcRenderer } from 'electron'

export interface HanaApi {
  getPlatform: () => Promise<string>
  getAppVersion: () => Promise<string>
  selectFolder: () => Promise<string | null>
  selectFiles: (options?: { filters?: { name: string; extensions: string[] }[] }) => Promise<string[]>
  readFile: (filePath: string) => Promise<string | null>
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
  onUpdateAvailable: (cb: (info: unknown) => void) => void
  onUpdateNotAvailable: (cb: () => void) => void
  onUpdateDownloaded: (cb: () => void) => void
  onUpdateError: (cb: (msg: string) => void) => void
  getLoomDir: () => Promise<string>
}

contextBridge.exposeInMainWorld('hana', {
  getPlatform: () => ipcRenderer.invoke('get-platform'),
  getAppVersion: () => ipcRenderer.invoke('get-app-version'),
  selectFolder: () => ipcRenderer.invoke('select-folder'),
  selectFiles: (options?: { filters?: { name: string; extensions: string[] }[] }) =>
    ipcRenderer.invoke('select-files', options),
  readFile: (filePath: string) => ipcRenderer.invoke('read-file', filePath),
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
  onUpdateAvailable: (cb: (info: unknown) => void) => ipcRenderer.on('update-available', (_e, info) => cb(info)),
  onUpdateNotAvailable: (cb: () => void) => ipcRenderer.on('update-not-available', () => cb()),
  onUpdateDownloaded: (cb: () => void) => ipcRenderer.on('update-downloaded', () => cb()),
  onUpdateError: (cb: (msg: string) => void) => ipcRenderer.on('update-error', (_e, msg: string) => cb(msg)),
  getLoomDir: () => ipcRenderer.invoke('get-loom-dir'),
} satisfies HanaApi)
