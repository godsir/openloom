import { contextBridge, ipcRenderer } from 'electron'

contextBridge.exposeInMainWorld('petApi', {
  moveWindow: (dx: number, dy: number) => ipcRenderer.send('pet:move', dx, dy),
  showContextMenu: (x: number, y: number) => ipcRenderer.send('pet:context-menu', x, y),
  setIgnoreMouse: (ignore: boolean) => ipcRenderer.send('pet:set-ignore-mouse', ignore),
  setDnd: (on: boolean) => ipcRenderer.send('pet:set-dnd', on),
  getPositions: () => ipcRenderer.invoke('pet:get-positions'),
  onCommand: (cb: (cmd: string) => void) => {
    ipcRenderer.on('pet:command', (_e, cmd: string) => cb(cmd))
  },
})
