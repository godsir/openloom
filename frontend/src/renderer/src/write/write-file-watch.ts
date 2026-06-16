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

let pollTimer: ReturnType<typeof setInterval> | null = null;

/**
 * Start watching a file for external changes using polling.
 * Compares file content to detect external modifications.
 */
export function startWatchingFile(options: FileWatchOptions): void {
  stopWatchingFile();

  let lastContent: string | null = null;

  const poll = async () => {
    try {
      const result: { ok: boolean; content: string; size: number; truncated: boolean } =
        await loomRpc('vfs.read_file', {
          path: options.filePath,
          workspace_root: options.workspaceRoot,
        });

      if (!result.ok) return;

      // First poll: just set baseline without notifying
      if (lastContent === null) {
        lastContent = result.content;
        return;
      }

      // Compare content to detect external changes
      if (result.content !== lastContent) {
        lastContent = result.content;
        options.onContentChanged(result.content, result.size, result.truncated);
      }
    } catch {
      // Silently ignore poll errors
    }
  };

  // Poll every 2 seconds
  pollTimer = setInterval(poll, 2000);
}

export function stopWatchingFile(): void {
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
