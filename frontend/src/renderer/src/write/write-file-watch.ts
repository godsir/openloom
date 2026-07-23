// External file watch — detects changes from outside the editor and reloads content

import { loomRpc } from '../services/jsonrpc';
import type { WriteFileKind } from '../stores/write';

interface FileWatchOptions {
  filePath: string;
  workspaceRoot: string;
  fileKind: WriteFileKind;
  onContentChanged: (content: string, size: number, truncated: boolean) => boolean;
  onImageChanged: (dataUrl: string) => void;
  onError: (error: string) => void;
}

let pollTimer: ReturnType<typeof setInterval> | null = null;
let watchGeneration = 0;

/**
 * Start watching a file for external changes using polling.
 * Compares file content to detect external modifications.
 */
export function startWatchingFile(options: FileWatchOptions): void {
  stopWatchingFile();
  const generation = ++watchGeneration;

  let lastContent: string | null = null;
  let pollInFlight = false;

  const poll = async () => {
    if (pollInFlight) return;
    pollInFlight = true;
    try {
      const result: { ok: boolean; content: string; size: number; truncated: boolean } =
        await loomRpc('vfs.read_file', {
          path: options.filePath,
          workspace_root: options.workspaceRoot,
        });

      if (generation !== watchGeneration || !result.ok) return;

      // First poll: just set baseline without notifying
      if (lastContent === null) {
        lastContent = result.content;
        return;
      }

      // Compare content to detect external changes
      if (result.content !== lastContent) {
        const accepted = options.onContentChanged(result.content, result.size, result.truncated);
        if (accepted) lastContent = result.content;
      }
    } catch {
      // Silently ignore poll errors
    } finally {
      pollInFlight = false;
    }
  };

  // Poll every 2 seconds
  pollTimer = setInterval(poll, 2000);
}

export function stopWatchingFile(): void {
  watchGeneration += 1;
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}
