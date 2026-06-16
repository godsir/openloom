// Mode-switch synchronization between TipTap and CodeMirror
// Preserves cursor position during Rich ↔ Source transitions

import type { Editor } from '@tiptap/react';
import { tipTapJsonToMarkdown, markdownToTipTapJson } from './markdown-projection';

/**
 * Extract current Markdown projection from a TipTap editor.
 */
export function extractMarkdownFromTipTap(editor: Editor): string {
  const json = editor.getJSON();
  return tipTapJsonToMarkdown(json);
}

/**
 * Load Markdown content into a TipTap editor.
 * Attempts to restore cursor position after content replacement.
 */
export function loadMarkdownToTipTap(editor: Editor, markdown: string): void {
  const { from, to } = editor.state.selection;
  const doc = markdownToTipTapJson(markdown);
  editor.commands.setContent(doc);
  // Try to restore cursor position
  try {
    const safeFrom = Math.min(from, editor.state.doc.content.size);
    const safeTo = Math.min(to, editor.state.doc.content.size);
    editor.commands.setTextSelection({ from: safeFrom, to: safeTo });
  } catch {
    // Fallback: cursor at end of document
    editor.commands.setTextSelection(editor.state.doc.content.size);
  }
}

/**
 * Sync Markdown content to a CodeMirror EditorView.
 * Replaces the full document while trying to preserve cursor position.
 */
export function syncMarkdownToCodeMirror(
  editorView: { state: { selection: { main: { head: number } }; doc: { length: number } }; dispatch: (tr: unknown) => void },
  markdown: string
): void {
  const currentPos = editorView.state.selection.main.head;
  const currentLen = editorView.state.doc.length;
  (editorView as any).dispatch({
    changes: { from: 0, to: currentLen, insert: markdown },
    selection: { anchor: Math.min(currentPos, markdown.length) },
  });
}
