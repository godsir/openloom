// External file watch — detects changes from outside the editor and reloads content

import { loomRpc } from '../services/jsonrpc';
import type { WriteFileKind } from '../stores/write';

interface FileWatchOptions {
  filePath: string;
  workspaceRoot: string;
  fileKind: WriteFileKind;
  onContentChanged: (content: string, size: number, truncated: boolean) => void;
  onImageChanged: (dataUrl: string) => void;
  onError: (error: string) => void;
}

let activeWatcherDispose: (() => void) | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;

/**
 * Start watching a file for external changes using polling.
 * Falls back to interval-based polling if native watch is unavailable.
 */
export function startWatchingFile(options: FileWatchOptions): void {
  stopWatchingFile();

  let lastModified = 0;

  const poll = async () => {
    try {
      const result: { ok: boolean; size?: number; modified?: number } = await loomRpc(
        'vfs.read_file',
        { path: options.filePath, workspace_root: options.workspaceRoot }
      );

      if (!result.ok) return;

      const mtime = result.modified ?? 0;
      if (mtime > lastModified) {
        lastModified = mtime;

        if (options.fileKind === 'image') {
          // Reload image as data URL
          const dataUrl = await readImageAsDataUrl(options.filePath, options.workspaceRoot);
          options.onImageChanged(dataUrl);
        } else {
          const readResult: { ok: boolean; content: string; size: number; truncated: boolean } =
            await loomRpc('vfs.read_file', {
              path: options.filePath,
              workspace_root: options.workspaceRoot,
            });
          if (readResult.ok) {
            options.onContentChanged(readResult.content, readResult.size, readResult.truncated);
          }
        }
      }
    } catch {
      // Silently ignore poll errors
    }
  };

  // Poll every 2 seconds
  pollTimer = setInterval(poll, 2000);
}

export function stopWatchingFile(): void {
  if (activeWatcherDispose) {
    activeWatcherDispose();
    activeWatcherDispose = null;
  }
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

async function readImageAsDataUrl(filePath: string, workspaceRoot: string): Promise<string> {
  try {
    const result: { ok: boolean; dataUrl?: string } = await loomRpc('vfs.read_image', {
      path: filePath,
      workspace_root: workspaceRoot,
    });
    if (result.ok && result.dataUrl) return result.dataUrl;
  } catch {}
  return '';
}
