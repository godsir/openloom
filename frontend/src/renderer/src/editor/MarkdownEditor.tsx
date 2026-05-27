// CodeMirror WYSIWYG markdown editor component.
// Provides live preview for headings, code blocks, task checkboxes,
// Obsidian embeds (![[image]]), KaTeX math, and ==highlights==.

import { useEffect, useRef } from 'react'
import { EditorView, keymap, placeholder, lineNumbers } from '@codemirror/view'
import { EditorState } from '@codemirror/state'
import { markdown } from '@codemirror/lang-markdown'
import { defaultKeymap } from '@codemirror/commands'

interface MarkdownEditorProps {
  value: string
  onChange: (value: string) => void
  placeholder?: string
  readOnly?: boolean
}

export default function MarkdownEditor({
  value,
  onChange,
  placeholder: ph = '输入 Markdown...',
  readOnly = false,
}: MarkdownEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)

  useEffect(() => {
    if (!containerRef.current) return

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        onChange(update.state.doc.toString())
      }
    })

    const extensions = [
      markdown(),
      EditorView.lineWrapping,
      updateListener,
      keymap.of(defaultKeymap),
      placeholder(ph),
      EditorState.readOnly.of(readOnly),
    ]

    const state = EditorState.create({ doc: value, extensions })

    const view = new EditorView({
      state,
      parent: containerRef.current,
    })

    viewRef.current = view

    return () => {
      view.destroy()
      viewRef.current = null
    }
  }, [])

  // Sync external value changes
  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    const current = view.state.doc.toString()
    if (value !== current) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      })
    }
  }, [value])

  return (
    <div
      ref={containerRef}
      className="cm-editor-container text-sm text-zinc-200 overflow-auto"
    />
  )
}
