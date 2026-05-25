import type { PreviewItem } from '../types';

export type { PreviewItem };

export interface PreviewSlice {
  previewItems: PreviewItem[];
  activePreviewTabId: string | null;
  /** @deprecated — compat alias for activePreviewTabId */
  activeTabId: string | null;
  /** @deprecated — compat: tab IDs derived from previewItems that should remain open */
  openTabs: string[];
}

const EMPTY_PREVIEW_ITEMS: PreviewItem[] = [];

export function createPreviewSlice() {
  return {
    previewItems: EMPTY_PREVIEW_ITEMS,
    activePreviewTabId: null,
    activeTabId: null,
    openTabs: [],
  };
}

/** Stable selector — returns same [] reference when store.previewItems is empty */
export function selectPreviewItems(s: PreviewSlice): PreviewItem[] {
  return s.previewItems ?? EMPTY_PREVIEW_ITEMS;
}

export function selectActiveTabId(s: PreviewSlice): string | null {
  return s.activePreviewTabId ?? null;
}

export function selectOpenTabs(s: PreviewSlice): string[] {
  return s.openTabs ?? [];
}

export function selectMarkdownPreviewIds(s: PreviewSlice): string[] {
  return s.previewItems
    .filter(item => item.type === 'markdown' || item.language === 'markdown')
    .map(item => item.id);
}
