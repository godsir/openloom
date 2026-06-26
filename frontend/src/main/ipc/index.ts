import { registerFileIpc } from './files'
import { registerShellIpc } from './shell'
import { registerAppIpc } from './app'
import { registerWriteIpc } from './write'
import { registerImIpc } from './im'

export function registerIpcHandlers(): void {
  registerFileIpc()
  registerShellIpc()
  registerAppIpc()
  registerWriteIpc()
  registerImIpc()
}
