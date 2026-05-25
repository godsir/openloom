import type { PreviewItem } from '../types';

export function openPreview(_item: PreviewItem): void {}
export function closePreview(): void {}
export function closeTab(_tabId: string): void {}
export function setActiveTab(_tabId: string | null): void {}
export function canSpawnViewer(_item: PreviewItem | null): boolean { return false; }
export function spawnViewer(_file: PreviewItem): void {}
export function upsertPreviewItem(_item: PreviewItem): void {}
export function setMarkdownPreviewActive(_id: string | null, _active?: boolean): void {}
export function selectMarkdownPreviewIds(_s: unknown): string[] { return []; }
