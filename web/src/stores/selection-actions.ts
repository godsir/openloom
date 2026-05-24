import { useStore } from './index';
import type { PreviewItem } from '../types';
import type { EditorView } from '@codemirror/view';
import type { FloatingAnchorRect } from './input-slice';
import type { ChatMessage } from './chat-types';

const MAX_QUOTED_SELECTION_CHARS = 2000;

let quotedSelectionLifecycle:
  | { target: Document; cleanup: () => void }
  | null = null;

export function initQuotedSelectionLifecycle(target: Document = document): () => void {
  if (quotedSelectionLifecycle?.target === target) {
    return quotedSelectionLifecycle.cleanup;
  }
  quotedSelectionLifecycle?.cleanup();

  const handleSelectionChange = () => {
    clearSelectionIfNativeSelectionIsEmpty(target);
  };
  target.addEventListener('selectionchange', handleSelectionChange);

  const cleanup = () => {
    target.removeEventListener('selectionchange', handleSelectionChange);
    if (quotedSelectionLifecycle?.target === target) {
      quotedSelectionLifecycle = null;
    }
  };
  quotedSelectionLifecycle = { target, cleanup };
  return cleanup;
}

/**
 * 捕获 previewItem 中的文本选中。
 * CM 模式传入 cmView，DOM 模式不传。
 */
export function captureSelection(previewItem: PreviewItem, cmView?: EditorView): void {
  if (cmView) {
    captureCMSelection(previewItem, cmView);
  } else {
    captureDOMSelection(previewItem);
  }
}

function captureCMSelection(previewItem: PreviewItem, view: EditorView): void {
  const { from, to } = view.state.selection.main;
  if (from === to) {
    clearSelection();
    return;
  }
  const rawText = view.state.sliceDoc(from, to);
  const text = rawText.trim();
  if (!text) {
    clearSelection();
    return;
  }
  const leadingTrimmed = rawText.length - rawText.trimStart().length;
  const trailingTrimmed = rawText.length - rawText.trimEnd().length;
  const textStart = from + leadingTrimmed;
  const textEnd = to - trailingTrimmed;
  const lineStart = view.state.doc.lineAt(textStart).number;
  const lineEnd = view.state.doc.lineAt(Math.max(textStart, textEnd - 1)).number;

  useStore.getState().setQuotedSelection({
    text,
    sourceTitle: previewItem.title,
    sourceKind: 'preview',
    sourceFilePath: previewItem.filePath,
    lineStart,
    lineEnd,
    charCount: text.length,
    anchorRect: getCMSelectionAnchorRect(view, textStart, textEnd),
    updatedAt: Date.now(),
  });
}

function captureDOMSelection(previewItem: PreviewItem): void {
  const sel = window.getSelection();
  const text = sel?.toString().trim();
  if (!text) {
    clearSelection();
    return;
  }
  const clipped = clipQuotedText(text);

  useStore.getState().setQuotedSelection({
    text: clipped,
    sourceTitle: previewItem.title,
    sourceKind: 'preview',
    charCount: text.length,
    anchorRect: sel && sel.rangeCount > 0 ? getRangeAnchorRect(sel.getRangeAt(0)) : undefined,
    updatedAt: Date.now(),
  });
}

export function captureChatSelection(sessionPath: string): void {
  const sel = window.getSelection();
  const text = sel?.toString().trim();
  if (!sel || !text || sel.rangeCount === 0) {
    clearSelection();
    return;
  }

  const anchorElement = nodeElement(sel.anchorNode);
  const focusElement = nodeElement(sel.focusNode);
  if (!anchorElement || !focusElement) return;
  if (isInteractiveSelectionElement(anchorElement) || isInteractiveSelectionElement(focusElement)) return;

  const anchorMessage = closestMessageElement(anchorElement);
  const focusMessage = closestMessageElement(focusElement);
  if (!anchorMessage || !focusMessage || anchorMessage !== focusMessage) return;

  const messageId = anchorMessage.dataset.messageId;
  if (!messageId) return;

  const message = findMessage(sessionPath, messageId);
  if (!message) return;

  useStore.getState().setQuotedSelection({
    text: clipQuotedText(text),
    sourceTitle: message.role === 'assistant' ? 'Assistant message' : 'User message',
    sourceKind: 'chat',
    sourceSessionPath: sessionPath,
    sourceMessageId: message.id,
    sourceRole: message.role,
    charCount: text.length,
    anchorRect: getRangeAnchorRect(sel.getRangeAt(0)),
    updatedAt: Date.now(),
  });
}

function clipQuotedText(text: string): string {
  return text.length > MAX_QUOTED_SELECTION_CHARS ? text.slice(0, MAX_QUOTED_SELECTION_CHARS) : text;
}

function nodeElement(node: Node | null): Element | null {
  if (!node) return null;
  if (node.nodeType === Node.ELEMENT_NODE) return node as Element;
  return node.parentElement;
}

function closestMessageElement(element: Element): HTMLElement | null {
  return element.closest<HTMLElement>('[data-message-id]');
}

function isInteractiveSelectionElement(element: Element): boolean {
  return !!element.closest('input, textarea, select, button, [contenteditable="true"], [data-selection-ignore="true"], [data-mobile-gesture-ignore="true"]');
}

function findMessage(sessionPath: string, messageId: string): ChatMessage | null {
  const session = useStore.getState().chatSessions[sessionPath];
  if (!session) return null;
  for (const item of session.items) {
    if (item.type === 'message' && item.data.id === messageId) return item.data;
  }
  return null;
}

function toPlainRect(rect: DOMRect | ClientRect): FloatingAnchorRect {
  return {
    left: rect.left,
    right: rect.right,
    top: rect.top,
    bottom: rect.bottom,
    width: rect.width,
    height: rect.height,
  };
}

function unionRects(rects: Array<DOMRect | ClientRect>): FloatingAnchorRect | undefined {
  if (rects.length === 0) return undefined;
  let left = Number.POSITIVE_INFINITY;
  let right = Number.NEGATIVE_INFINITY;
  let top = Number.POSITIVE_INFINITY;
  let bottom = Number.NEGATIVE_INFINITY;
  for (const rect of rects) {
    left = Math.min(left, rect.left);
    right = Math.max(right, rect.right);
    top = Math.min(top, rect.top);
    bottom = Math.max(bottom, rect.bottom);
  }
  if (!Number.isFinite(left) || !Number.isFinite(right) || !Number.isFinite(top) || !Number.isFinite(bottom)) return undefined;
  return { left, right, top, bottom, width: right - left, height: bottom - top };
}

export function getRangeAnchorRect(range: Range): FloatingAnchorRect | undefined {
  const clientRects = typeof range.getClientRects === 'function'
    ? Array.from(range.getClientRects()).filter(rect => rect.width > 0 || rect.height > 0)
    : [];
  if (clientRects.length > 0) return unionRects(clientRects);

  if (typeof range.getBoundingClientRect !== 'function') return undefined;
  const rect = range.getBoundingClientRect();
  if (rect.width <= 0 && rect.height <= 0) return undefined;
  return toPlainRect(rect);
}

function getCMSelectionAnchorRect(view: EditorView, from: number, to: number): FloatingAnchorRect | undefined {
  const withCoords = view as EditorView & {
    coordsAtPos?: (pos: number, side?: -1 | 1) => DOMRect | null;
  };
  if (typeof withCoords.coordsAtPos !== 'function') return undefined;

  const start = withCoords.coordsAtPos(from, 1);
  const end = withCoords.coordsAtPos(to, -1) || withCoords.coordsAtPos(Math.max(from, to - 1), 1);
  const rects = [start, end].filter((rect): rect is DOMRect => !!rect);
  return unionRects(rects);
}

export function clearSelection(): void {
  const s = useStore.getState();
  if (s.quotedSelection) s.clearQuotedSelection();
}

function clearSelectionIfNativeSelectionIsEmpty(target: Document): void {
  const sel = getNativeSelection(target);
  const text = sel?.toString().trim();
  if (sel && text && sel.rangeCount > 0) return;
  clearSelection();
}

function getNativeSelection(target: Document): Selection | null {
  if (typeof target.getSelection === 'function') {
    return target.getSelection();
  }
  return target.defaultView?.getSelection?.() ?? window.getSelection();
}
