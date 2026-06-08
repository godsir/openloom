import { ipcMain, dialog } from 'electron'
import * as fs from 'fs'
import * as path from 'path'

export function registerWriteIpc(): void {
  // Pick workspace directory
  ipcMain.handle('write:pick-workspace', async () => {
    const result = await dialog.showOpenDialog({
      properties: ['openDirectory'],
      title: 'Select Write Workspace',
    })
    return { canceled: result.canceled, path: result.filePaths[0] || null }
  })

  // Export write document as HTML
  ipcMain.handle('write:export-html', async (_event, { filePath, content }: { filePath: string; content: string }) => {
    const html = `<!DOCTYPE html><html><head><meta charset="utf-8"><title>${path.basename(filePath)}</title>
<style>body{font-family:system-ui,sans-serif;max-width:800px;margin:40px auto;padding:0 20px;line-height:1.6;}
pre{background:#f4f4f4;padding:12px;border-radius:4px;overflow-x:auto;}
code{background:#f4f4f4;padding:2px 4px;border-radius:2px;}</style></head><body>${content}</body></html>`
    const result = await dialog.showSaveDialog({
      defaultPath: path.basename(filePath).replace(/\.md$/, '.html'),
      filters: [{ name: 'HTML', extensions: ['html'] }],
    })
    if (!result.canceled && result.filePath) {
      fs.writeFileSync(result.filePath, html, 'utf-8')
      return { ok: true, path: result.filePath }
    }
    return { ok: false, canceled: true }
  })

  // Read workspace image as data URL
  ipcMain.handle('write:read-image', async (_event, { filePath, workspaceRoot }: { filePath: string; workspaceRoot: string }) => {
    try {
      const fullPath = path.resolve(workspaceRoot, filePath)
      // Path traversal protection
      if (!fullPath.startsWith(path.resolve(workspaceRoot))) {
        return { ok: false, message: 'path outside workspace' }
      }
      const ext = path.extname(fullPath).toLowerCase()
      const mimeMap: Record<string, string> = {
        '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg',
        '.gif': 'image/gif', '.svg': 'image/svg+xml', '.webp': 'image/webp',
      }
      const mimeType = mimeMap[ext] || 'image/png'
      const data = fs.readFileSync(fullPath)
      const base64 = data.toString('base64')
      return { ok: true, dataUrl: `data:${mimeType};base64,${base64}`, mimeType }
    } catch (e: any) {
      return { ok: false, message: e.message }
    }
  })

  // Copy write document as rich text to clipboard
  ipcMain.handle('write:copy-rich-text', async (_event, { content }: { content: string }) => {
    try {
      const { clipboard } = require('electron')
      clipboard.write({ text: content, html: content })
      return { ok: true }
    } catch (e: any) {
      return { ok: false, message: e.message }
    }
  })

  // Watch file for external changes (stub — returns ok)
  ipcMain.handle('write:watch-file', async (_event, { filePath, workspaceRoot }: { filePath: string; workspaceRoot: string }) => {
    const fullPath = path.resolve(workspaceRoot, filePath)
    if (!fullPath.startsWith(path.resolve(workspaceRoot))) {
      return { ok: false }
    }
    // File watching is a stub for now — actual fs.watch implementation deferred
    return { ok: true }
  })

  // Unwatch file (stub)
  ipcMain.handle('write:unwatch-file', async () => {
    return { ok: true }
  })
}
