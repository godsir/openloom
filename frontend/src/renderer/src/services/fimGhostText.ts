import { Prec, StateEffect, StateField, type Extension } from '@codemirror/state'
import { Decoration, EditorView, keymap, ViewPlugin, WidgetType } from '@codemirror/view'
import { useWriteStore } from '../stores/write'
import { requestFimCompletion } from './completion'

interface GhostSuggestion {
  from: number
  text: string
}

const setGhost = StateEffect.define<GhostSuggestion | null>()

class GhostWidget extends WidgetType {
  constructor(readonly text: string) { super() }

  toDOM(): HTMLElement {
    const span = document.createElement('span')
    span.className = 'cm-fimGhost'
    span.textContent = this.text
    span.setAttribute('aria-hidden', 'true')
    return span
  }

  eq(other: GhostWidget): boolean {
    return other.text === this.text
  }
}

const ghostField = StateField.define<GhostSuggestion | null>({
  create: () => null,
  update(value, transaction) {
    if (transaction.docChanged || transaction.selection) value = null
    for (const effect of transaction.effects) {
      if (effect.is(setGhost)) value = effect.value
    }
    return value
  },
  provide: (field) => EditorView.decorations.from(field, (ghost) =>
    ghost
      ? Decoration.set([
          Decoration.widget({
            widget: new GhostWidget(ghost.text),
            side: 1,
          }).range(ghost.from),
        ])
      : Decoration.none
  ),
})

const ghostPlugin = ViewPlugin.fromClass(class {
  private timer: ReturnType<typeof setTimeout> | null = null
  private requestId = 0

  constructor(private view: EditorView) {
    this.schedule()
  }

  update(update: { docChanged: boolean; selectionSet: boolean; focusChanged: boolean }) {
    if (update.docChanged || update.selectionSet || update.focusChanged) this.schedule()
  }

  private clear() {
    if (this.timer) clearTimeout(this.timer)
    this.timer = null
    this.requestId += 1
    if (this.view.state.field(ghostField, false)) {
      this.view.dispatch({ effects: setGhost.of(null) })
    }
  }

  private schedule() {
    this.clear()
    if (!this.view.hasFocus || !this.view.state.selection.main.empty) return

    const pos = this.view.state.selection.main.head
    const doc = this.view.state.doc.toString()
    const prefix = doc.slice(Math.max(0, pos - 8_000), pos)
    if (prefix.length < 10) return
    const suffix = doc.slice(pos, Math.min(doc.length, pos + 4_000))
    const snapshot = doc
    const requestId = ++this.requestId
    const config = useWriteStore.getState()

    this.timer = setTimeout(async () => {
      this.timer = null
      const result = await requestFimCompletion(prefix, suffix, config.shortMaxTokens)
      if (requestId !== this.requestId || !result.ok || !result.completion) return
      const current = this.view.state
      if (
        current.doc.toString() !== snapshot ||
        !current.selection.main.empty ||
        current.selection.main.head !== pos ||
        !this.view.hasFocus
      ) return

      const firstLine = result.completion.replace(/\r\n/g, '\n').split('\n')[0]
      if (!firstLine) return
      this.view.dispatch({ effects: setGhost.of({ from: pos, text: firstLine }) })
    }, config.shortDebounceMs)
  }

  destroy() {
    this.clear()
  }
})

export function buildFimGhostTextExtension(): Extension {
  return [
    ghostField,
    ghostPlugin,
    Prec.high(keymap.of([
      {
        key: 'Tab',
        run: (view) => {
          const ghost = view.state.field(ghostField, false)
          if (!ghost) return false
          view.dispatch({
            changes: { from: ghost.from, insert: ghost.text },
            selection: { anchor: ghost.from + ghost.text.length },
          })
          return true
        },
      },
      {
        key: 'Escape',
        run: (view) => {
          if (!view.state.field(ghostField, false)) return false
          view.dispatch({ effects: setGhost.of(null) })
          return true
        },
      },
    ])),
    EditorView.theme({
      '.cm-fimGhost': {
        color: 'var(--text-muted)',
        opacity: '0.55',
        pointerEvents: 'none',
        whiteSpace: 'pre',
      },
    }),
  ]
}
