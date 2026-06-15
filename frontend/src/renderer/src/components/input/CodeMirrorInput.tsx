import React, { useRef, useEffect, useCallback } from 'react'
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view'
import { EditorState, Compartment } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
import { autocompletion } from '@codemirror/autocomplete'
import { useStore } from '../../stores'
import { buildFimCompletionSource } from '../../services/fimSource'

const fimCompletionSource = buildFimCompletionSource()

interface CodeMirrorInputProps {
  value: string
  onChange: (value: string) => void
  onSend: () => void
  onPaste?: (e: ClipboardEvent) => void
  placeholder?: string
  disabled?: boolean
}

export const CodeMirrorInput: React.FC<CodeMirrorInputProps> = ({ value, onChange, onSend, onPaste, placeholder, disabled }) => {
  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const fimEnabled = useStore(s => s.fimEnabled)

  // Compartments for dynamic reconfiguration — preserves undo history, cursor, scroll
  const fimCompartment = useRef(new Compartment())
  const editableCompartment = useRef(new Compartment())

  // Create editor once on mount — never destroyed until unmount
  useEffect(() => {
    if (!containerRef.current) return

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        const newValue = update.state.doc.toString()
        onChange(newValue)
      }
    })

    const fimExtension = fimEnabled
      ? autocompletion({
          override: [fimCompletionSource],
          defaultKeymap: true,
          activateOnTyping: false,   // chat: only trigger manually (Ctrl+Space), don't pop up while typing
        })
      : []

    const extensions: any[] = [
      lineNumbers(),
      highlightActiveLine(),
      history(),
      markdown(),
      syntaxHighlighting(defaultHighlightStyle),
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        {
          key: 'Enter',
          run: () => {
            onSend()
            return true
          },
          shift: () => {
            // Shift+Enter: pass through to defaultKeymap which inserts newline
            return false
          },
        },
        { key: 'Mod-Enter', run: () => { onSend(); return true } },
      ]),
      updateListener,
      EditorView.theme({
        '&': { maxHeight: '200px', overflow: 'auto' },
        '.cm-content': { fontFamily: 'var(--font-mono)', fontSize: '14px', padding: '8px 12px' },
        '.cm-line': { lineHeight: '1.6' },
        '.cm-gutters': {
          background: 'var(--bg-surface)',
          color: 'var(--text-muted)',
          borderRight: '1px solid var(--border)',
        },
      }),
      // Compartments for dynamic reconfiguration
      editableCompartment.current.of(EditorView.editable.of(!disabled)),
      fimCompartment.current.of(fimExtension),
    ]

    const state = EditorState.create({
      doc: value,
      extensions,
    })

    const view = new EditorView({
      state,
      parent: containerRef.current,
    })

    viewRef.current = view

    return () => {
      view.destroy()
      viewRef.current = null
    }
  }, []) // Created once — never destroyed until unmount

  // Toggle FIM autocompletion dynamically
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: fimCompartment.current.reconfigure(
        fimEnabled
          ? autocompletion({
              override: [fimCompletionSource],
              defaultKeymap: true,
              activateOnTyping: false,
            })
          : []
      ),
    })
  }, [fimEnabled])

  // Toggle editable state dynamically
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: editableCompartment.current.reconfigure(
        EditorView.editable.of(!disabled)
      ),
    })
  }, [disabled])

  // Sync external value changes back to editor
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    const currentDoc = view.state.doc.toString()
    if (currentDoc !== value) {
      view.dispatch({
        changes: { from: 0, to: currentDoc.length, insert: value }
      })
    }
  }, [value])

  // Handle image paste — intercept before CodeMirror processes it
  const handleContainerPaste = useCallback((e: ClipboardEvent) => {
    const items = e.clipboardData?.items
    if (!items) return
    for (let i = 0; i < items.length; i++) {
      if (items[i].type.startsWith('image/')) {
        onPaste?.(e)
        break
      }
    }
  }, [onPaste])

  useEffect(() => {
    const container = containerRef.current
    if (!container || !onPaste) return
    container.addEventListener('paste', handleContainerPaste)
    return () => container.removeEventListener('paste', handleContainerPaste)
  }, [handleContainerPaste, onPaste])

  return (
    <div
      ref={containerRef}
      style={{
        border: '1px solid var(--border)',
        borderRadius: 8,
        overflow: 'hidden',
        minHeight: 60,
      }}
    />
  )
}
