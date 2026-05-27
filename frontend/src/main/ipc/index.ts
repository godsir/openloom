import { registerFileIpc } from './files'
import { registerShellIpc } from './shell'
import { registerAppIpc } from './app'

export function registerIpcHandlers(): void {
  registerFileIpc()
  registerShellIpc()
  registerAppIpc()
}
