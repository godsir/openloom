import { ipcMain, shell } from 'electron'
import { resolve } from 'path'
import { existsSync } from 'fs'

// Only these URL schemes may be handed to the OS to open externally. Blocks
// file:, smb:, and custom protocol-handler schemes that a malicious link
// (LLM output / remote marketplace entry) could otherwise trigger.
const SAFE_EXTERNAL_SCHEMES = new Set(['http:', 'https:', 'mailto:'])

function isSafeExternalUrl(url: unknown): url is string {
  if (typeof url !== 'string') return false
  try {
    return SAFE_EXTERNAL_SCHEMES.has(new URL(url).protocol)
  } catch {
    return false
  }
}

export function registerShellIpc(): void {
  ipcMain.handle('open-external', async (_, url: string) => {
    if (!isSafeExternalUrl(url)) {
      throw new Error('blocked: unsafe url scheme')
    }
    await shell.openExternal(url)
  })

  ipcMain.handle('open-folder', async (_, filePath: string) => {
    if (typeof filePath !== 'string' || !filePath) throw new Error('invalid path')
    const resolved = resolve(filePath)
    if (!existsSync(resolved)) throw new Error('path does not exist: ' + resolved)
    await shell.openPath(resolved)
  })

  ipcMain.handle('open-file', async (_, filePath: string) => {
    if (typeof filePath !== 'string' || !filePath) throw new Error('invalid path')
    await shell.openPath(resolve(filePath))
  })
}
