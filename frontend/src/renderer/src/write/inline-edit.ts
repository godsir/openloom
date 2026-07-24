// AI Inline Edit Pipeline
// Constructs structured prompts, sends to LLM, parses responses for diff review

import type { WriteEditorSelectionState } from '../stores/write';

// ============================================================
// Types
// ============================================================

export interface WriteInlineEditScope {
  kind: 'selection' | 'paragraph' | 'line';
  text: string;
  from: number;
  to: number;
  lineFrom: number;
  lineTo: number;
  prefix: string;
  suffix: string;
}

export interface WriteInlineEditRequest {
  prefix: string;
  editScope: string;
  suffix: string;
  instruction: string;
  filePath: string;
  recentEdits?: string[];
}

export interface WriteInlineEditResult {
  replacement: string;
  scope: WriteInlineEditScope;
}

// ============================================================
// Constants
// ============================================================

const PREFIX_WINDOW = 3000; // chars before selection
const SUFFIX_WINDOW = 2000; // chars after selection

// ============================================================
// Scope resolution
// ============================================================

export function resolveEditScope(
  content: string,
  selection: WriteEditorSelectionState,
  granularity: 'selection' | 'paragraph' | 'line' = 'selection',
): WriteInlineEditScope {
  const { from, to, lineFrom, lineTo } = selection;

  let scopeFrom = from;
  let scopeTo = to;

  if (granularity === 'paragraph') {
    // Expand to paragraph boundaries
    const lines = content.split('\n');
    let start = lineFrom;
    let end = lineTo;
    while (start > 0 && lines[start - 1].trim() !== '') start--;
    while (end < lines.length - 1 && lines[end + 1].trim() !== '') end++;
    // Convert line indices back to char offsets
    let charCount = 0;
    for (let i = 0; i < lines.length; i++) {
      if (i === start) scopeFrom = charCount;
      charCount += lines[i].length + 1;
      if (i === end) scopeTo = charCount - 1;
    }
  } else if (granularity === 'line') {
    const lines = content.split('\n');
    let charCount = 0;
    for (let i = 0; i < lines.length; i++) {
      if (i === lineFrom) scopeFrom = charCount;
      charCount += lines[i].length + 1;
      if (i === lineTo) scopeTo = charCount - 1;
    }
  }

  const prefixStart = Math.max(0, scopeFrom - PREFIX_WINDOW);
  const suffixEnd = Math.min(content.length, scopeTo + SUFFIX_WINDOW);

  return {
    kind: granularity,
    text: content.slice(scopeFrom, scopeTo),
    from: scopeFrom,
    to: scopeTo,
    lineFrom,
    lineTo,
    prefix: content.slice(prefixStart, scopeFrom),
    suffix: content.slice(scopeTo, suffixEnd),
  };
}

// ============================================================
// Prompt construction
// ============================================================

export function buildInlineEditPrompt(request: WriteInlineEditRequest): string {
  const parts: string[] = [];

  parts.push('<<<PREFIX');
  parts.push(request.prefix || '(start of document)');
  parts.push('');
  parts.push('<<<EDIT_SCOPE');
  parts.push(request.editScope);
  parts.push('');
  parts.push('<<<SUFFIX');
  parts.push(request.suffix || '(end of document)');
  parts.push('---');
  parts.push(`Instruction: ${request.instruction}`);
  parts.push('');
  parts.push('Only modify the text within EDIT_SCOPE. Keep the PREFIX and SUFFIX unchanged.');
  parts.push('Return your edit wrapped in <<<EDIT and <<</EDIT markers.');

  if (request.recentEdits && request.recentEdits.length > 0) {
    parts.push('');
    parts.push('Recent edits to this document:');
    request.recentEdits.forEach((e, i) => parts.push(`${i + 1}. ${e}`));
  }

  return parts.join('\n');
}

// ============================================================
// Response parsing
// ============================================================

export function parseInlineEditResponse(response: string): string | null {
  const editMatch = response.match(/<<<EDIT\s*([\s\S]*?)\s*<<<\/EDIT/);
  if (editMatch) {
    // 空 EDIT 块视为解析失败——否则会把选区静默替换为空字符串（误删内容）
    const text = editMatch[1].trim();
    return text.length > 0 ? text : null;
  }

  // Fallback: try SHORT marker
  const shortMatch = response.match(/<<<SHORT\s*([\s\S]*?)\s*<<<\/SHORT/);
  if (shortMatch) {
    const text = shortMatch[1].trim();
    return text.length > 0 ? text : null;
  }

  // Fallback: try LONG marker
  const longMatch = response.match(/<<<LONG\s*([\s\S]*?)\s*<<<\/LONG/);
  if (longMatch) {
    const text = longMatch[1].trim();
    return text.length > 0 ? text : null;
  }

  // Last resort: return the raw response if it looks like plain text
  const cleaned = response.trim();
  if (cleaned.length > 0 && !cleaned.includes('<<<')) {
    return cleaned;
  }

  return null;
}

// ============================================================
// Replacement application
// ============================================================

export function applyInlineEditReplacement(
  content: string,
  scope: WriteInlineEditScope,
  replacement: string,
): string {
  const before = content.slice(0, scope.from);
  const after = content.slice(scope.to);
  return before + replacement + after;
}

// ============================================================
// Diff chunk generation (for red/green review)
// ============================================================

export interface SimpleDiffChunk {
  id: string;
  originalText: string;
  modifiedText: string;
  fromA: number;
  toA: number;
  fromB: number;
  toB: number;
  accepted: boolean | null;
}

export function buildDiffChunks(
  _content: string,
  scope: WriteInlineEditScope,
  replacement: string,
): SimpleDiffChunk[] {
  const id = `diff-${scope.from}-${scope.to}`;
  return [
    {
      id,
      originalText: scope.text,
      modifiedText: replacement,
      fromA: scope.from,
      toA: scope.to,
      fromB: scope.from,
      toB: scope.from + replacement.length,
      accepted: null,
    },
  ];
}
