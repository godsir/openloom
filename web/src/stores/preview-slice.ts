import type { FileRef } from '../types/file-ref';

export interface PreviewItem {
  id: string;
  title: string;
  filePath?: string;
  type?: string;
  content?: string;
}

export interface PreviewSlice {
  previewItems: PreviewItem[];
  activePreviewTabId: string | null;
}

const EMPTY_PREVIEW_ITEMS: PreviewItem[] = [];

export function createPreviewSlice() {
  return {
    previewItems: EMPTY_PREVIEW_ITEMS,
    activePreviewTabId: null,
  };
}

/** Stable selector — returns same [] reference when store.previewItems is empty */
export function selectPreviewItems(s: PreviewSlice): PreviewItem[] {
  return s.previewItems ?? EMPTY_PREVIEW_ITEMS;
}

export function selectActiveTabId(s: PreviewSlice): string | null {
  return s.activePreviewTabId ?? null;
}
