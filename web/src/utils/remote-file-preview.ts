import type { FileRef } from '../types/file-ref';

export function isWebRuntime(): boolean {
  return document.documentElement.getAttribute('data-platform') === 'web';
}

export function fileRefDownloadUrl(file: FileRef): string {
  if (file.resource?.links.content) return file.resource.links.content;
  if (file.path) return `/api/files/download?path=${encodeURIComponent(file.path)}`;
  return '';
}

export function openFileRefPreview(file: FileRef, opts?: { origin?: string; sessionPath?: string; messageId?: string; blockIdx?: number }): void {
  const url = fileRefDownloadUrl(file);
  if (!url) return;
  window.open(url, '_blank');
}

export function openMobileWorkbenchPreview(opts: { file: { name: string; isDir?: boolean }; subdir: string; rootId: string }): void {
  // Stub: mobile workbench preview not yet implemented in web PWA
}
