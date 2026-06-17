import React, { useRef, useEffect } from 'react'
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, placeholder as cmPlaceholder } from '@codemirror/view'
import { EditorState, Compartment } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
import { autocompletion } from '@codemirror/autocomplete'
import { closeBrackets } from '@codemirror/autocomplete'
import { useStore } from '../../stores'
import { buildFimCompletionSource } from '../../services/fimSource'

const fimCompletionSource = buildFimCompletionSource()

interface CodeMirrorEditorProps {
  value: string
  onChange: (value: string) => void
  placeholder?: string
  fontSize?: number
}

/**
 * Full-height CodeMirror 6 editor for write mode.
 * Features: markdown syntax highlighting, line numbers, FIM autocompletion,
 * bracket closing, history, standard editor keybindings (Enter = newline).
 * Uses Compartments for dynamic extension reconfiguration without editor rebuild.
 */
export const CodeMirrorEditor: React.FC<CodeMirrorEditorProps> = ({
  value,
  onChange,
  placeholder = '',
  fontSize = 14,
}) => {
  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)

  // Compartments for dynamic reconfiguration — preserves undo history, cursor, scroll
  const fimCompartment = useRef(new Compartment())
  const themeCompartment = useRef(new Compartment())

  const fimEnabled = useStore(s => s.fimEnabled)

  // Build the static theme once
  const baseTheme = EditorView.theme({
    '&': {
      height: '100%',
      overflow: 'auto',
      background: 'var(--bg)',
    },
    '.cm-content': {
      fontFamily: 'var(--font-mono)',
      padding: '16px',
      lineHeight: '1.8',
      minHeight: '100%',
    },
    '.cm-gutters': {
      background: 'var(--bg-surface)',
      color: 'var(--text-muted)',
      borderRight: '1px solid var(--border)',
      lineHeight: '1.8',
    },
    '.cm-activeLine': {
      background: 'var(--bg-active, rgba(255,255,255,0.04))',
    },
    '.cm-activeLineGutter': {
      background: 'var(--bg-active, rgba(255,255,255,0.04))',
    },
  })

  // Build the dynamic theme from fontSize
  // 使用绝对 px lineHeight 确保 .cm-gutters 与 .cm-content 行高一致，防止行号错位
  const dynamicTheme = EditorView.theme({
    '.cm-content': {
      fontSize: `${fontSize}px`,
      lineHeight: `${Math.round(fontSize * 1.8)}px`,
    },
    '.cm-gutters': {
      fontSize: `${Math.max(10, fontSize - 2)}px`,
      lineHeight: `${Math.round(fontSize * 1.8)}px`,
    },
  })

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
          activateOnTyping: true,
        })
      : []

    const extensions: any[] = [
      lineNumbers(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      EditorView.lineWrapping,
      history(),
      markdown(),
      syntaxHighlighting(defaultHighlightStyle),
      closeBrackets(),
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        indentWithTab,
        // Enter inserts newline (no send behavior in write mode)
        {
          key: 'Enter',
          run: (view) => {
            view.dispatch(view.state.replaceSelection('\n'))
            return true
          },
        },
      ]),
      updateListener,
      EditorView.editable.of(true),
      baseTheme,
      cmPlaceholder(placeholder),
      // Compartments for dynamic reconfiguration
      themeCompartment.current.of(dynamicTheme),
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

  // Toggle FIM autocompletion dynamically (preserves undo history, cursor, scroll)
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: fimCompartment.current.reconfigure(
        fimEnabled
          ? autocompletion({
              override: [fimCompletionSource],
              defaultKeymap: true,
              activateOnTyping: true,
            })
          : []
      ),
    })
  }, [fimEnabled])

  // Update font size dynamically (preserves undo history, cursor, scroll)
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    // 使用绝对 px lineHeight 确保 .cm-gutters 与 .cm-content 行高一致，防止行号错位
    const newTheme = EditorView.theme({
      '.cm-content': {
        fontSize: `${fontSize}px`,
        lineHeight: `${Math.round(fontSize * 1.8)}px`,
      },
      '.cm-gutters': {
        fontSize: `${Math.max(10, fontSize - 2)}px`,
        lineHeight: `${Math.round(fontSize * 1.8)}px`,
      },
    })
    view.dispatch({
      effects: themeCompartment.current.reconfigure(newTheme),
    })
  }, [fontSize])

  // Sync external value changes back to editor
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    const currentDoc = view.state.doc.toString()
    if (currentDoc !== value) {
      view.dispatch({
        changes: { from: 0, to: currentDoc.length, insert: value },
      })
    }
  }, [value])

  return (
    <div
      ref={containerRef}
      style={{
        height: '100%',
        width: '100%',
        overflow: 'hidden',
      }}
    />
  )
}
