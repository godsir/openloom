import { registerFileIpc } from './files'
import { registerShellIpc } from './shell'
import { registerAppIpc } from './app'
import { registerWriteIpc } from './write'

export function registerIpcHandlers(): void {
  registerFileIpc()
  registerShellIpc()
  registerAppIpc()
  registerWriteIpc()
}
