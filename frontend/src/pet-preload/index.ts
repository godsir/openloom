import { contextBridge, ipcRenderer } from 'electron'

contextBridge.exposeInMainWorld('petApi', {
  moveWindow: (dx: number, dy: number) => ipcRenderer.send('pet:move', dx, dy),
  showContextMenu: (x: number, y: number) => ipcRenderer.send('pet:context-menu', x, y),
  onCommand: (cb: (cmd: string) => void) => {
    ipcRenderer.on('pet:command', (_e, cmd: string) => cb(cmd))
  },
})
