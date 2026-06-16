// Thread registry — maps file paths to AI session thread IDs
// Used by WriteAssistantPanel to know which session belongs to which file

const STORAGE_KEY = 'loom:writeFileThreads';

export function loadThreadRegistry(): Record<string, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

export function saveThreadRegistry(registry: Record<string, string>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(registry));
  } catch {}
}

export function getThreadForFile(
  registry: Record<string, string>,
  filePath: string,
): string | null {
  return registry[filePath] ?? null;
}

export function setThreadForFile(
  registry: Record<string, string>,
  filePath: string,
  threadId: string,
): Record<string, string> {
  const next = { ...registry, [filePath]: threadId };
  saveThreadRegistry(next);
  return next;
}

export function removeThreadForFile(
  registry: Record<string, string>,
  filePath: string,
): Record<string, string> {
  const next = { ...registry };
  delete next[filePath];
  saveThreadRegistry(next);
  return next;
}
