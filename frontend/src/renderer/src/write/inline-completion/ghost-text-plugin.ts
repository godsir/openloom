// Ghost Text Completion — Copilot-style inline suggestions
// CM6 ViewPlugin that shows grey ghost text at cursor, accepts with Tab

import {
  Decoration,
  DecorationSet,
  EditorView,
  ViewPlugin,
  ViewUpdate,
  WidgetType,
} from '@codemirror/view';
import { RangeSetBuilder, StateField, StateEffect } from '@codemirror/state';
import { requestFimCompletion, getFimConfig } from '../../services/completion';

// ============================================================
// Ghost Text Widget
// ============================================================

class GhostTextWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }

  eq(other: GhostTextWidget): boolean {
    return other.text === this.text;
  }

  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-ghost-text';
    span.textContent = this.text;
    span.style.color = 'var(--text-muted, #666)';
    span.style.opacity = '0.5';
    span.style.pointerEvents = 'none';
    span.style.userSelect = 'none';
    return span;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

// ============================================================
// State
// ============================================================

interface GhostState {
  suggestion: string | null;
  cursorPos: number;
  loading: boolean;
  abortController: AbortController | null;
}

const setGhostSuggestion = StateEffect.define<{ suggestion: string | null; cursorPos: number }>();
const setGhostLoading = StateEffect.define<boolean>();

const ghostStateField = StateField.define<GhostState>({
  create(): GhostState {
    return { suggestion: null, cursorPos: 0, loading: false, abortController: null };
  },
  update(state, tr) {
    let next = { ...state };
    for (const effect of tr.effects) {
      if (effect.is(setGhostSuggestion)) {
        next.suggestion = effect.value.suggestion;
        next.cursorPos = effect.value.cursorPos;
        next.loading = false;
      }
      if (effect.is(setGhostLoading)) {
        next.loading = effect.value;
      }
    }
    // Clear suggestion on any document change or cursor move
    if (tr.docChanged || tr.selection) {
      next.suggestion = null;
      next.loading = false;
    }
    return next;
  },
});

// ============================================================
// Plugin
// ============================================================

let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let pendingAbort: AbortController | null = null;

function requestGhostCompletion(view: EditorView, debounceMs: number = 300) {
  if (debounceTimer) clearTimeout(debounceTimer);
  if (pendingAbort) { pendingAbort.abort(); pendingAbort = null; }

  debounceTimer = setTimeout(async () => {
    const pos = view.state.selection.main.head;
    const doc = view.state.doc;
    const prefix = doc.sliceString(0, pos);
    const suffix = doc.sliceString(pos, doc.length);

    // Skip if prefix too short
    if (prefix.length < 3) return;

    view.dispatch({ effects: setGhostLoading.of(true) });

    try {
      const result = await requestFimCompletion(prefix, suffix, 64);
      if (result.ok && result.completion) {
        const suggestion = result.completion;
        view.dispatch({
          effects: setGhostSuggestion.of({ suggestion, cursorPos: pos }),
        });
      }
    } catch {
      // Silently ignore errors
    } finally {
      view.dispatch({ effects: setGhostLoading.of(false) });
    }
  }, debounceMs);
}

const ghostTextPlugin = ViewPlugin.fromClass(
  class GhostPlugin {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = Decoration.none;
      // Initial completion request
      requestGhostCompletion(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.selectionSet) {
        // Clear current suggestion
        this.decorations = Decoration.none;

        // Request new completion on cursor movement or typing
        if (update.selectionSet || (update.docChanged && update.changes.desc.iterChangedRanges)) {
          requestGhostCompletion(update.view);
        }
      }

      // Apply new suggestion
      const state = update.view.state.field(ghostStateField, false);
      if (state?.suggestion && !update.state.selection.main.empty) {
        // Don't show ghost when there's a selection
        this.decorations = Decoration.none;
      } else if (state?.suggestion) {
        const cursorPos = update.state.selection.main.head;
        const builder = new RangeSetBuilder<Decoration>();
        builder.add(
          cursorPos,
          cursorPos,
          Decoration.widget({ widget: new GhostTextWidget(state.suggestion), side: 1 }),
        );
        this.decorations = builder.finish();
      } else {
        this.decorations = Decoration.none;
      }
    }
  },
  {
    decorations: (plugin) => plugin.decorations,
  },
);

// ============================================================
// Key handler — Tab to accept
// ============================================================

function ghostAcceptKeyHandler(view: EditorView): boolean {
  const state = view.state.field(ghostStateField, false);
  if (!state?.suggestion) return false;

  const cursorPos = state.cursorPos;
  view.dispatch({
    changes: { from: cursorPos, insert: state.suggestion },
    selection: { anchor: cursorPos + state.suggestion.length },
    effects: setGhostSuggestion.of({ suggestion: null, cursorPos: -1 }),
  });
  return true;
}

// ============================================================
// Export — full extension array
// ============================================================

export function ghostTextExtensions() {
  return [
    ghostStateField,
    ghostTextPlugin,
    // Tab key handler for accepting ghost text
    EditorView.domEventHandlers({
      keydown: (event, view) => {
        if (event.key === 'Tab' && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
          const accepted = ghostAcceptKeyHandler(view);
          if (accepted) {
            event.preventDefault();
            event.stopPropagation();
          }
        }
      },
    }),
  ];
}

export { ghostStateField, setGhostSuggestion, setGhostLoading };
