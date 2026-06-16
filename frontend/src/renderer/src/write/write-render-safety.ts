// Render safety guards — prevent expensive rendering on large/truncated files

export const SAFE_RENDER_MAX_CHARS = 300_000;
export const READ_ONLY_MAX_CHARS = 1_000_000;

export type RenderNotice = 'none' | 'large-file' | 'truncated';

export interface RenderSafetyResult {
  livePreviewEnabled: boolean;
  richPreviewEnabled: boolean;
  markdownPreviewEnabled: boolean;
  readOnly: boolean;
  notice: RenderNotice;
}

interface RenderSafetyOptions {
  isMarkdown: boolean;
  contentLength: number;
  fileSize: number;
  truncated: boolean;
}

export function getRenderSafety(options: RenderSafetyOptions): RenderSafetyResult {
  const { isMarkdown, contentLength, fileSize, truncated } = options;
  const effectiveSize = Math.max(contentLength, fileSize);

  // Truncated file — disable ALL rendering, force readOnly
  if (truncated) {
    return {
      livePreviewEnabled: false,
      richPreviewEnabled: false,
      markdownPreviewEnabled: false,
      readOnly: true,
      notice: 'truncated',
    };
  }

  // Non-markdown file
  if (!isMarkdown) {
    return {
      livePreviewEnabled: false,
      richPreviewEnabled: false,
      markdownPreviewEnabled: false,
      readOnly: effectiveSize > READ_ONLY_MAX_CHARS,
      notice: 'none',
    };
  }

  // Large file (>300K) — disable live/rich previews
  if (effectiveSize > SAFE_RENDER_MAX_CHARS) {
    return {
      livePreviewEnabled: false,
      richPreviewEnabled: false,
      markdownPreviewEnabled: true,
      readOnly: effectiveSize > READ_ONLY_MAX_CHARS,
      notice: 'large-file',
    };
  }

  // Normal file — everything enabled
  return {
    livePreviewEnabled: true,
    richPreviewEnabled: true,
    markdownPreviewEnabled: true,
    readOnly: false,
    notice: 'none',
  };
}
