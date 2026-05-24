import type { StoreState } from '../stores';

export interface UiContextPayload {
  currentViewed: string | null;
  activeFile: string | null;
  activePreview: string | null;
  pinnedFiles: string[];
}

export function collectUiContext(state: StoreState): UiContextPayload | null {
  const currentViewed = (state as any).deskCurrentPath || null;

  // activeTabId / pinnedViewers not yet ported from Hanako — stub for now
  const activeFile: string | null = null;
  const activePreview: string | null = null;
  const pinnedFiles: string[] = [];

  if (
    !currentViewed &&
    !activeFile &&
    !activePreview &&
    pinnedFiles.length === 0
  ) {
    return null;
  }

  return { currentViewed, activeFile, activePreview, pinnedFiles };
}
