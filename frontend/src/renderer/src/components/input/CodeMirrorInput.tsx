import React, { useRef, useEffect, useCallback } from 'react'
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view'
import { EditorState } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
import { autocompletion, CompletionContext } from '@codemirror/autocomplete'
import { useStore } from '../../stores'
import { requestFimCompletion } from '../../services/completion'

function buildFimCompletionSource() {
  return async (context: CompletionContext) => {
    const appMode = useStore.getState().appMode
    if (appMode !== 'chat') return null

    const view = context.view
    const pos = context.pos
    const doc = view.state.doc.toString()
    const prefix = doc.slice(0, pos)
    const suffix = doc.slice(pos)

    // Skip if prefix is too short
    if (prefix.length < 10) return null

    try {
      const result = await requestFimCompletion(prefix, suffix, 64)
      if (result.ok && result.completion) {
        const text = result.completion.trim()
        if (text.length === 0) return null
        return {
          from: pos,
          to: pos,
          options: [{
            label: text,
            type: 'text',
            apply: text,
          }],
          filter: false,
        }
      }
    } catch { /* silent */ }
    return null
  }
}

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

  useEffect(() => {
    if (!containerRef.current) return

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        const newValue = update.state.doc.toString()
        onChange(newValue)
      }
    })

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
      EditorView.editable.of(!disabled),
      EditorView.theme({
        '&': { maxHeight: '200px', overflow: 'auto' },
        '.cm-content': { fontFamily: 'var(--font-mono)', fontSize: '14px', padding: '8px 12px' },
        '.cm-line': { lineHeight: '1.6' },
      }),
    ]

    // Only add autocompletion when FIM is enabled
    if (fimEnabled) {
      extensions.push(autocompletion({
        override: [fimCompletionSource],
        defaultKeymap: false,
        activateOnTyping: false,
      }))
    }

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
  }, [fimEnabled, disabled]) // Re-create when toggles change

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
