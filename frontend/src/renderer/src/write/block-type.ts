// Block type detection and conversion for selection toolbar
// Detects current block type from a line, converts between types

export type WriteBlockType =
  | 'paragraph'
  | 'heading1'
  | 'heading2'
  | 'heading3'
  | 'quote'
  | 'bullet'
  | 'ordered'
  | 'code';

const BLOCK_PATTERNS: { type: WriteBlockType; regex: RegExp }[] = [
  { type: 'code', regex: /^```/ },
  { type: 'heading1', regex: /^#\s+/ },
  { type: 'heading2', regex: /^##\s+/ },
  { type: 'heading3', regex: /^###\s+/ },
  { type: 'quote', regex: /^>\s+/ },
  { type: 'bullet', regex: /^[-*+]\s+/ },
  { type: 'ordered', regex: /^\d+[.)]\s+/ },
];

export function detectBlockType(line: string): WriteBlockType {
  for (const { type, regex } of BLOCK_PATTERNS) {
    if (regex.test(line)) return type;
  }
  return 'paragraph';
}

const MARKER_MAP: Record<WriteBlockType, { prefix: string; stripRegex: RegExp }> = {
  paragraph:    { prefix: '',              stripRegex: /^[-*+#>\d]+\s*/ },
  heading1:     { prefix: '# ',            stripRegex: /^#{1,3}\s+/ },
  heading2:     { prefix: '## ',           stripRegex: /^#{1,3}\s+/ },
  heading3:     { prefix: '### ',          stripRegex: /^#{1,3}\s+/ },
  quote:        { prefix: '> ',            stripRegex: /^>\s*/ },
  bullet:       { prefix: '- ',            stripRegex: /^[-*+\d.]+\s*/ },
  ordered:      { prefix: '1. ',           stripRegex: /^[-*+\d.]+\s*/ },
  code:         { prefix: '```\n',         stripRegex: /.*/ },
};

export function applyBlockType(lines: string[], type: WriteBlockType): string[] {
  if (type === 'code') {
    if (lines.length === 0) return ['```\n\n```'];
    return ['```', ...lines.map(l => l.replace(/^```/, '')), '```'];
  }

  const marker = MARKER_MAP[type];
  return lines.map((line, i) => {
    const trimmed = line.trimStart();
    const indent = line.slice(0, line.length - trimmed.length);
    const stripped = trimmed.replace(marker.stripRegex, '');

    if (type === 'ordered') {
      return `${indent}${i + 1}. ${stripped}`;
    }
    return `${indent}${marker.prefix}${stripped}`;
  });
}
