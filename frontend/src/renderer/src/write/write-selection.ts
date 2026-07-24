// Selection state utilities — comparison, creation, serialization

import type { WriteEditorSelectionState } from '../stores/write';

export function createSelectionState(
  text: string,
  from: number,
  to: number,
  lineFrom: number,
  lineTo: number,
  blockType: string | null = null,
  containsImage: boolean = false,
  source: WriteEditorSelectionState['source'] = 'markdown',
): WriteEditorSelectionState {
  return { source, text, from, to, lineFrom, lineTo, blockType, containsImage };
}

export function writeSelectionStatesEqual(
  a: WriteEditorSelectionState | null,
  b: WriteEditorSelectionState | null,
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return (
    a.text === b.text &&
    a.from === b.from &&
    a.to === b.to &&
    a.lineFrom === b.lineFrom &&
    a.lineTo === b.lineTo &&
    a.blockType === b.blockType &&
    a.containsImage === b.containsImage
  );
}

export function isSelectionEmpty(sel: WriteEditorSelectionState | null): boolean {
  if (!sel) return true;
  return sel.from === sel.to || sel.text.length === 0;
}

export function getSelectionRange(
  content: string,
  sel: WriteEditorSelectionState,
): { before: string; selected: string; after: string } {
  return {
    before: content.slice(0, sel.from),
    selected: content.slice(sel.from, sel.to),
    after: content.slice(sel.to),
  };
}
