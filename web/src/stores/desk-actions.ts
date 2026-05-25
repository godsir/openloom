import { useStore } from './index';
import { hanaFetch } from '../hooks/use-hana-fetch';
import { loomRpc } from '../adapter';
import { hasServerConnection } from '../services/server-connection';
import {
  persistCurrentWorkspaceUiStateNow,
  loadPersistedWorkspaceUiState,
  hydratePersistedPreviewItems,
} from './workspace-ui-state-actions';
import type { DeskFile, PreviewItem } from '../types';
// @ts-expect-error — shared JS module
import { normalizeWorkspacePath } from '../../../../shared/workspace-history.js';

// ── Helpers ──

function selectActiveWorkspaceDir(): string | null {
  const { selectedFolder, homeFolder } = useStore.getState();
  return selectedFolder || homeFolder || null;
}

async function postFilesApi(dir: string, body: Record<string, unknown>): Promise<{ ok: boolean; files?: DeskFile[]; error?: string }> {
  try {
    const res = await hanaFetch('/api/desk/files', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ dir, ...body }),
    });
    return await res.json();
  } catch (e) {
    console.error('[desk] postFilesApi failed:', e);
    return { ok: false, error: String(e) };
  }
}

async function fetchDeskFiles(dir: string, subdir?: string): Promise<{ files: DeskFile[]; basePath: string; error?: string }> {
  const params = new URLSearchParams();
  params.set('dir', dir);
  if (subdir) params.set('subdir', subdir);
  try {
    const res = await hanaFetch(`/api/desk/files?${params.toString()}`);
    const data = await res.json();
    if (data.error) return { files: [], basePath: dir, error: data.error };
    return {
      files: Array.isArray(data.files) ? data.files : [],
      basePath: data.basePath || dir,
    };
  } catch (e) {
    console.error('[desk] fetchDeskFiles failed:', e);
    return { files: [], basePath: dir, error: String(e) };
  }
}

// ── Public API ──

export function toggleJianSidebar(_open?: boolean) {
  const s = useStore.getState();
  const next = typeof _open === 'boolean' ? _open : !s.jianDrawerOpen;
  useStore.setState({ jianDrawerOpen: next });
}

export async function activateWorkspaceDesk(cwd?: string | null, opts?: { reload?: boolean }): Promise<void> {
  const s = useStore.getState();
  const activeDir = cwd || selectActiveWorkspaceDir();
  if (!activeDir) return;

  const previousRoot = s.deskBasePath;
  if (previousRoot && previousRoot !== activeDir && opts?.reload !== false) {
    await persistCurrentWorkspaceUiStateNow(previousRoot);
  }

  useStore.setState({
    deskBasePath: activeDir,
    selectedFolder: activeDir,
  });

  // Sync workspace cwd to backend so LLM sees correct working directory
  loomRpc('workspace.set_cwd', { cwd: activeDir }).catch(() => {});

  if (opts?.reload !== false) {
    const { files, basePath } = await fetchDeskFiles(activeDir);
    useStore.setState({
      deskBasePath: basePath,
      deskFiles: files,
      deskCurrentPath: '',
      deskExpandedPaths: [],
      deskSelectedPath: '',
    });
  }

  // Restore persisted UI state for this workspace
  if (hasServerConnection(s)) {
    const persisted = await loadPersistedWorkspaceUiState(activeDir);
    if (persisted) {
      const patch: Record<string, unknown> = {};
      if (persisted.deskCurrentPath !== undefined) patch.deskCurrentPath = persisted.deskCurrentPath;
      if (persisted.deskExpandedPaths) patch.deskExpandedPaths = persisted.deskExpandedPaths;
      if (persisted.deskSelectedPath !== undefined) patch.deskSelectedPath = persisted.deskSelectedPath;
      if (persisted.jianView !== undefined) patch.jianView = persisted.jianView;
      if (persisted.jianDrawerOpen !== undefined) patch.jianDrawerOpen = persisted.jianDrawerOpen;
      if (Object.keys(patch).length > 0) useStore.setState(patch as never);
    }
  }
}

export async function loadDeskFiles(opts?: {
  overridePath?: string;
  subdir?: string;
  /** 是否刷新当前目录（而非子目录） */
  refreshCurrent?: boolean;
  /** 同时设置当前路径 */
  setCurrentPath?: string;
}): Promise<DeskFile[]> {
  const s = useStore.getState();
  const dir = opts?.overridePath || selectActiveWorkspaceDir();
  if (!dir) return [];

  const { files, basePath } = await fetchDeskFiles(dir, opts?.subdir);

  if (opts?.subdir) {
    useStore.setState((prev: any) => ({
      deskBasePath: basePath,
      deskTreeFilesByPath: {
        ...prev.deskTreeFilesByPath,
        [opts.subdir!]: files,
      },
    }));
  } else {
    useStore.setState({
      deskBasePath: basePath,
      deskFiles: files,
      deskTreeFilesByPath: { '': files },
      deskCurrentPath: opts?.setCurrentPath || '',
    });
  }
  return files;
}

export async function applyFolder(path: string): Promise<void> {
  const s = useStore.getState();
  const normalized = normalizeWorkspacePath(path);
  if (!normalized) return;

  // Save current workspace UI state before switching
  const prevRoot = s.deskBasePath;
  if (prevRoot && prevRoot !== normalized) {
    await persistCurrentWorkspaceUiStateNow(prevRoot);
  }

  useStore.setState({ selectedFolder: normalized, deskBasePath: normalized, deskFiles: [] });

  // Sync workspace cwd to backend so LLM sees correct working directory
  loomRpc('workspace.set_cwd', { cwd: normalized }).catch(() => {});

  // Persist workspace history
  try {
    const histRes = await hanaFetch('/api/config/workspaces/recent', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: normalized }),
    });
    const histData = await histRes.json().catch(() => ({}));
    if (Array.isArray(histData?.cwd_history)) {
      useStore.setState({ cwdHistory: histData.cwd_history });
    }
  } catch {}

  const { files, basePath } = await fetchDeskFiles(normalized);
  useStore.setState({
    deskBasePath: basePath,
    deskFiles: files,
    deskCurrentPath: '',
    deskSelectedPath: '',
  });

  // Remove from workspaceFolders if it was an extra folder (promoted to primary)
  const extraFolders = (s.workspaceFolders || []).filter(f => f !== normalized);
  useStore.setState({ workspaceFolders: extraFolders });

  // Restore persisted UI state
  if (hasServerConnection(s)) {
    const persisted = await loadPersistedWorkspaceUiState(normalized);
    if (persisted) {
      const patch: Record<string, unknown> = {};
      if (persisted.deskCurrentPath !== undefined) patch.deskCurrentPath = persisted.deskCurrentPath;
      if (persisted.deskExpandedPaths) patch.deskExpandedPaths = persisted.deskExpandedPaths;
      if (persisted.deskSelectedPath !== undefined) patch.deskSelectedPath = persisted.deskSelectedPath;
      if (persisted.jianView !== undefined) patch.jianView = persisted.jianView;
      if (persisted.jianDrawerOpen !== undefined) patch.jianDrawerOpen = persisted.jianDrawerOpen;
      if (Object.keys(patch).length > 0) useStore.setState(patch as never);
    }
  }
}

export function addWorkspaceFolder(path: string): void {
  const normalized = normalizeWorkspacePath(path);
  if (!normalized) return;
  const s = useStore.getState();
  const current = s.workspaceFolders || [];
  if (current.includes(normalized) || normalized === s.selectedFolder || normalized === s.homeFolder) return;
  useStore.setState({ workspaceFolders: [...current, normalized] });
}

export function removeWorkspaceFolder(path: string): void {
  const s = useStore.getState();
  const current = s.workspaceFolders || [];
  useStore.setState({ workspaceFolders: current.filter(f => f !== path) });
}

export async function searchDeskFiles(query: string): Promise<any[]> {
  const s = useStore.getState();
  const dir = s.deskBasePath || selectActiveWorkspaceDir();
  if (!dir || !query) return [];
  try {
    const params = new URLSearchParams();
    params.set('dir', dir);
    params.set('q', query);
    const res = await hanaFetch(`/api/desk/search-files?${params.toString()}`);
    const data = await res.json();
    return Array.isArray(data.results) ? data.results : [];
  } catch (e) {
    console.error('[desk] searchDeskFiles failed:', e);
    return [];
  }
}

// ── File operations ──

async function refreshAfterMutation(dir: string, parentSubdir?: string, affectedSubdirs?: string[]): Promise<void> {
  const s = useStore.getState();
  // Refresh root files
  const { files } = await fetchDeskFiles(dir);
  const patch: Record<string, unknown> = { deskFiles: files };

  // Refresh affected subdirectory caches
  const treeFilesByPath = { ...s.deskTreeFilesByPath };
  treeFilesByPath[''] = files;

  if (parentSubdir) {
    const { files: subFiles } = await fetchDeskFiles(dir, parentSubdir);
    treeFilesByPath[parentSubdir] = subFiles;
  }

  if (affectedSubdirs) {
    for (const sub of affectedSubdirs) {
      const { files: subFiles } = await fetchDeskFiles(dir, sub);
      treeFilesByPath[sub] = subFiles;
    }
  }

  patch.deskTreeFilesByPath = treeFilesByPath;
  useStore.setState(patch as never);
}

export async function deskCreateFileInSubdir(
  parentSubdir: string,
  name: string,
  content: string,
): Promise<boolean> {
  const s = useStore.getState();
  const dir = s.deskBasePath || selectActiveWorkspaceDir();
  if (!dir || !name) return false;

  const result = await postFilesApi(dir, {
    action: 'create',
    subdir: parentSubdir,
    name,
    content,
  });

  if (result.ok && result.files) {
    useStore.setState((prev: any) => ({
      deskTreeFilesByPath: {
        ...prev.deskTreeFilesByPath,
        [parentSubdir]: result.files,
      },
    }));
  }

  return !!result.ok;
}

export async function deskMkdirInSubdir(
  parentSubdir: string,
  name: string,
): Promise<boolean> {
  const s = useStore.getState();
  const dir = s.deskBasePath || selectActiveWorkspaceDir();
  if (!dir || !name) return false;

  const result = await postFilesApi(dir, {
    action: 'mkdir',
    subdir: parentSubdir,
    name,
  });

  if (result.ok && result.files) {
    useStore.setState((prev: any) => ({
      deskTreeFilesByPath: {
        ...prev.deskTreeFilesByPath,
        [parentSubdir]: result.files,
      },
    }));
    // Also update root if parentSubdir is the current view
    if (parentSubdir === '') {
      useStore.setState({ deskFiles: result.files });
    }
  }

  return !!result.ok;
}

export async function deskRenameTreeItem(
  parentSubdir: string,
  oldName: string,
  newName: string,
  isDir: boolean,
): Promise<boolean> {
  const s = useStore.getState();
  const dir = s.deskBasePath || selectActiveWorkspaceDir();
  if (!dir || !oldName || !newName || oldName === newName) return false;

  const result = await postFilesApi(dir, {
    action: 'rename',
    subdir: parentSubdir,
    oldName,
    newName,
  });

  if (result.ok && result.files) {
    const prev = useStore.getState().deskTreeFilesByPath;
    useStore.setState({
      deskTreeFilesByPath: {
        ...prev,
        [parentSubdir]: result.files,
      },
    });
  }

  return !!result.ok;
}

export async function deskMoveTreeItem(
  sourceSubdir: string,
  sourceName: string,
  targetSubdir: string,
): Promise<boolean> {
  const s = useStore.getState();
  const dir = s.deskBasePath || selectActiveWorkspaceDir();
  if (!dir || !sourceName) return false;

  const result = await postFilesApi(dir, {
    action: 'move',
    subdir: sourceSubdir,
    name: sourceName,
    targetSubdir,
  });

  if (result.ok && result.files) {
    await refreshAfterMutation(dir, undefined, [sourceSubdir, targetSubdir]);
  }

  return !!result.ok;
}

export async function deskSafeDeleteTreeItem(
  subdir: string,
  name: string,
  isDir: boolean,
): Promise<boolean> {
  const s = useStore.getState();
  const dir = s.deskBasePath || selectActiveWorkspaceDir();
  if (!dir || !name) return false;

  const result = await postFilesApi(dir, {
    action: 'safeDelete',
    subdir,
    name,
  });

  if (result.ok && result.files) {
    useStore.setState((prev: any) => ({
      deskTreeFilesByPath: {
        ...prev.deskTreeFilesByPath,
        [subdir]: result.files,
      },
    }));
    // Also update root files
    const { files } = await fetchDeskFiles(dir);
    useStore.setState({
      deskFiles: files,
      deskTreeFilesByPath: {
        ...useStore.getState().deskTreeFilesByPath,
        '': files,
      },
    });
  }

  return !!result.ok;
}

// ── Jian editor ──

export async function loadJianContent(): Promise<string | null> {
  try {
    const res = await hanaFetch('/api/desk/jian');
    const data = await res.json();
    useStore.setState({ deskJianContent: data?.content || null });
    return data?.content || null;
  } catch {
    return null;
  }
}

export async function saveJianContent(content: string): Promise<boolean> {
  try {
    const res = await hanaFetch('/api/desk/jian', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content }),
    });
    const data = await res.json();
    return !!data.ok;
  } catch {
    return false;
  }
}

// ── Aliases for component compatibility ──
export { loadDeskFiles as loadDeskTreeFiles };
export const deskCreateFile = deskCreateFileInSubdir;
export const deskMoveTreeFiles = deskMoveTreeItem;

// ── Not yet wired — placeholder stubs ──
export function deskCurrentDir(): string { return useStore.getState().deskCurrentPath || ''; }
export async function deskUploadFiles(files: File[]): Promise<void> {}
export async function deskUploadFilesToSubdir(_subdir: string, _files: File[]): Promise<void> {}
export async function deskTrashTreeItems(_items: { subdir: string; name: string; isDir: boolean }[]): Promise<boolean> { return false; }
export function jumpToDeskSearchResult(_result: any): void {}

// Re-export for external use
export { buildPersistedWorkspaceUiState } from './workspace-ui-state-actions';