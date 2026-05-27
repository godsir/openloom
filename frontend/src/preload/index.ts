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
} satisfies HanaApi)
