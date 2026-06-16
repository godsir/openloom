import { ipcMain, dialog } from 'electron'
import * as fs from 'fs'
import * as path from 'path'

let BrowserWindow: any;

function wrapExportHtml(markdown: string, title: string): string {
  // Basic HTML wrapper with print-friendly CSS
  // The actual markdown rendering will be done in the renderer,
  // this receives pre-rendered HTML
  return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>${title || 'Document'}</title>
  <style>
    @media print {
      body { margin: 0; }
      @page { margin: 2cm 1.5cm; }
    }
    body {
      font-family: system-ui, -apple-system, 'Microsoft YaHei', sans-serif;
      font-size: 14px;
      line-height: 1.8;
      color: #333;
      max-width: 800px;
      margin: 0 auto;
      padding: 40px 24px;
    }
    h1 { font-size: 1.8em; border-bottom: 1px solid #eee; padding-bottom: 8px; }
    h2 { font-size: 1.5em; }
    h3 { font-size: 1.3em; }
    img { max-width: 100%; border-radius: 4px; }
    code { background: #f5f5f5; padding: 2px 6px; border-radius: 3px; font-family: 'Cascadia Code', monospace; font-size: 0.9em; }
    pre { background: #f5f5f5; padding: 16px; border-radius: 6px; overflow-x: auto; }
    pre code { background: none; padding: 0; }
    blockquote { border-left: 3px solid #ddd; margin: 0; padding: 4px 16px; color: #666; }
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #ddd; padding: 8px 12px; text-align: left; }
    th { background: #f5f5f5; }
  </style>
</head>
<body>
${markdown}
</body>
</html>`;
}

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

  // Read workspace binary file as base64 (for PDF etc.)
  ipcMain.handle('write:read-binary', async (_event, { filePath, workspaceRoot }: { filePath: string; workspaceRoot: string }) => {
    try {
      const fullPath = path.resolve(workspaceRoot, filePath)
      if (!fullPath.startsWith(path.resolve(workspaceRoot))) {
        return { ok: false, message: 'path outside workspace' }
      }
      const data = fs.readFileSync(fullPath)
      const base64 = data.toString('base64')
      return { ok: true, data: base64, size: data.length }
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

  // Export PDF — render HTML in hidden BrowserWindow then print to PDF
  ipcMain.handle('write:export-pdf', async (_event, markdown: string, title: string) => {
    const { BrowserWindow: BW } = await import('electron');
    const { dialog: dlg } = await import('electron');

    const result = await dlg.showSaveDialog({
      title: '导出 PDF',
      defaultPath: `${title || 'document'}.pdf`,
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    });

    if (result.canceled || !result.filePath) return { ok: false, error: 'cancelled' };

    // Create a hidden window for PDF rendering
    const win = new BW({
      width: 800,
      height: 600,
      show: false,
      webPreferences: { sandbox: true },
    });

    try {
      const html = wrapExportHtml(markdown, title);
      await win.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(html)}`);
      const pdfData = await win.webContents.printToPDFAsync({
        printBackground: true,
        margins: { top: '20mm', bottom: '20mm', left: '15mm', right: '15mm' },
      });
      fs.writeFileSync(result.filePath, pdfData);
      return { ok: true, path: result.filePath };
    } catch (err: any) {
      return { ok: false, error: err.message };
    } finally {
      win.destroy();
    }
  });

  // Export DOCX — convert markdown to HTML then to DOCX
  ipcMain.handle('write:export-docx', async (_event, markdown: string, title: string) => {
    const { dialog: dlg } = await import('electron');

    const result = await dlg.showSaveDialog({
      title: '导出 DOCX',
      defaultPath: `${title || 'document'}.docx`,
      filters: [{ name: 'Word Document', extensions: ['docx'] }],
    });

    if (result.canceled || !result.filePath) return { ok: false, error: 'cancelled' };

    try {
      // Use html-to-docx — imported dynamically
      const htmlToDocx = await import('html-to-docx');
      const html = wrapExportHtml(markdown, title);
      const docxBuffer = await htmlToDocx.default(html, null, {
        title: title || 'Document',
        margins: { top: 1440, bottom: 1440, left: 1080, right: 1080 },
      });
      fs.writeFileSync(result.filePath, Buffer.from(docxBuffer as ArrayBuffer));
      return { ok: true, path: result.filePath };
    } catch (err: any) {
      return { ok: false, error: err.message };
    }
  });

  // Enhanced export HTML
  ipcMain.handle('write:export-html-enhanced', async (_event, markdown: string, title: string) => {
    const { dialog: dlg } = await import('electron');

    const result = await dlg.showSaveDialog({
      title: '导出 HTML',
      defaultPath: `${title || 'document'}.html`,
      filters: [{ name: 'HTML', extensions: ['html', 'htm'] }],
    });

    if (result.canceled || !result.filePath) return { ok: false, error: 'cancelled' };

    try {
      const html = wrapExportHtml(markdown, title);
      fs.writeFileSync(result.filePath, html, 'utf-8');
      return { ok: true, path: result.filePath };
    } catch (err: any) {
      return { ok: false, error: err.message };
    }
  });

  // Watch file for external changes
  ipcMain.handle('write:watch-file', async (_event, filePath: string, workspaceRoot: string) => {
    const resolved = path.resolve(workspaceRoot, filePath);
    if (!resolved.startsWith(path.resolve(workspaceRoot))) {
      return { ok: false, error: 'path traversal rejected' };
    }

    try {
      const watcher = fs.watch(resolved, (eventType) => {
        if (eventType === 'change') {
          // Send notification to renderer
          const { BrowserWindow: BW } = require('electron');
          const win = BW.getAllWindows()[0];
          if (win) win.webContents.send('write:file-changed', filePath);
        }
      });

      // Store watcher for cleanup
      if (!(global as any).__writeWatchers) (global as any).__writeWatchers = {};
      (global as any).__writeWatchers[filePath] = watcher;

      return { ok: true };
    } catch (err: any) {
      return { ok: false, error: err.message };
    }
  });

  ipcMain.handle('write:unwatch-file', async (_event, filePath: string) => {
    const watchers = (global as any).__writeWatchers || {};
    if (watchers[filePath]) {
      watchers[filePath].close();
      delete watchers[filePath];
    }
    return { ok: true };
  });
}
