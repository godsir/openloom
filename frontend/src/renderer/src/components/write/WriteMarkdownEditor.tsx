// frontend/src/renderer/src/components/write/WriteMarkdownEditor.tsx
// 基于 CodeMirrorEditor 重构，适配 useWriteStore（替代主 store 的 fimEnabled）
import React, { useRef, useEffect } from 'react'
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, placeholder as cmPlaceholder } from '@codemirror/view'
import { EditorState, Compartment } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { syntaxHighlighting, defaultHighlightStyle } from '@codemirror/language'
import { closeBrackets } from '@codemirror/autocomplete'
import { useWriteStore } from '../../stores/write'
import { useStore } from '../../stores'
import { buildFimGhostTextExtension } from '../../services/fimGhostText'
import { createLivePreviewPlugin } from '../../write/markdown-live-preview'
import type { WritePreviewMode } from '../../stores/write'

interface WriteMarkdownEditorProps {
  value: string
  onChange: (value: string) => void
  placeholder?: string
  fontSize?: number
  /** When 'live', enables markdown syntax hiding via CM6 decorations */
  previewMode?: WritePreviewMode
  readOnly?: boolean
}

/**
 * Full-height CodeMirror 6 editor for write mode.
 * Features: markdown syntax highlighting, line numbers, FIM autocompletion,
 * bracket closing, history, standard editor keybindings (Enter = newline).
 * Uses Compartments for dynamic extension reconfiguration without editor rebuild.
 *
 * 与 CodeMirrorEditor 的区别：
 * - FIM 开关同时检查 write store 和主 store 的 fimEnabled
 * - 预留给 Live 装饰模式的扩展点
 */
export const WriteMarkdownEditor: React.FC<WriteMarkdownEditorProps> = ({
  value,
  onChange,
  placeholder = '',
  fontSize = 14,
  previewMode,
  readOnly = false,
}) => {
  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)

  // Compartments for dynamic reconfiguration — preserves undo history, cursor, scroll
  const fimCompartment = useRef(new Compartment())
  const themeCompartment = useRef(new Compartment())
  const liveCompartment = useRef(new Compartment())
  const editableCompartment = useRef(new Compartment())
  const fimGhostExtension = useRef(buildFimGhostTextExtension())

  // FIM 开关 — 同时检查 write store 和主 store (主 store 的 fimEnabled 是全局开关)
  const inlineCompletionEnabled = useWriteStore(s => s.inlineCompletionEnabled)
  const mainStoreFimEnabled = useStore(s => s.fimEnabled)
  const fimEnabled = inlineCompletionEnabled && mainStoreFimEnabled

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
      caretColor: 'var(--text)',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--text)',
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
      // Track selection for InlineAgent toolbar
      if (update.selectionSet) {
        const sel = update.state.selection.main
        const text = update.state.sliceDoc(sel.from, sel.to)
        if (text && sel.from !== sel.to) {
          const line = update.state.doc.lineAt(sel.from)
          useWriteStore.getState().setSelection({
            source: 'markdown',
            text,
            from: sel.from,
            to: sel.to,
            lineFrom: line.number - 1,
            lineTo: update.state.doc.lineAt(sel.to).number - 1,
            blockType: null,
            containsImage: false,
          })
        } else {
          useWriteStore.getState().setSelection(null)
        }
      }
    })

    const fimExtension = fimEnabled ? fimGhostExtension.current : []

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
      editableCompartment.current.of(EditorView.editable.of(!readOnly)),
      baseTheme,
      cmPlaceholder(placeholder),
      // Compartments for dynamic reconfiguration
      themeCompartment.current.of(dynamicTheme),
      fimCompartment.current.of(fimExtension),
      liveCompartment.current.of(previewMode === 'live' ? createLivePreviewPlugin() : []),
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
        fimEnabled ? fimGhostExtension.current : []
      ),
    })
  }, [fimEnabled])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: editableCompartment.current.reconfigure(EditorView.editable.of(!readOnly)),
    })
  }, [readOnly])

  // Toggle Live preview decorations
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: liveCompartment.current.reconfigure(
        previewMode === 'live' ? createLivePreviewPlugin() : []
      ),
    })
  }, [previewMode])

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
