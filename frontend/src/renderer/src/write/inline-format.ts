// Inline formatting toggle — bold, italic, strikethrough, code
// Applies/removes Markdown formatting markers on selected text

export type InlineFormatKind = 'bold' | 'italic' | 'strikethrough' | 'code';

const MARKERS: Record<InlineFormatKind, { marker: string; len: number }> = {
  bold:          { marker: '**', len: 2 },
  italic:        { marker: '*',  len: 1 },
  strikethrough: { marker: '~~', len: 2 },
  code:          { marker: '`',  len: 1 },
};

export function toggleInlineFormat(text: string, kind: InlineFormatKind): string | null {
  if (!text.trim()) return null;

  const { marker, len } = MARKERS[kind];
  const isWrapped = text.startsWith(marker) && text.endsWith(marker) && text.length >= len * 2;

  if (isWrapped) {
    // Remove formatting
    return text.slice(len, text.length - len);
  } else {
    // Add formatting
    return `${marker}${text}${marker}`;
  }
}

export function hasInlineFormat(text: string, kind: InlineFormatKind): boolean {
  const { marker, len } = MARKERS[kind];
  return text.startsWith(marker) && text.endsWith(marker) && text.length >= len * 2;
}
