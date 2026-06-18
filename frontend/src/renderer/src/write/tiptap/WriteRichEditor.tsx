import React, { useEffect, useRef, useCallback } from 'react';
import { useEditor, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import Image from '@tiptap/extension-image';
import Dropcursor from '@tiptap/extension-dropcursor';
import { markdownToTipTapJson, tipTapJsonToMarkdown } from './markdown-projection';
import { handleImagePaste, handleImageDrop } from './paste-image';
import { useWriteStore } from '../../stores/write';
import { useStore } from '../../stores';
import styles from './WriteRichEditor.module.css';

interface WriteRichEditorProps {
  value: string;
  onChange: (markdown: string) => void;
  fontSize?: number;
}

export const WriteRichEditor: React.FC<WriteRichEditorProps> = ({
  value,
  onChange,
  fontSize = 14,
}) => {
  const workspaceRoot = useWriteStore((s) => s.workspaceRoot);
  const lineHeight = useWriteStore((s) => s.lineHeight);
  const openLightbox = useStore((s) => s.openLightbox);

  // Guard against self-triggered content resets: when the editor fires
  // onUpdate we push markdown to the store; the store re-renders this
  // component with the same markdown; without a guard, the useEffect
  // would compare tipTapJsonToMarkdown(getJSON()) !== value and reset
  // the full document on every keystroke, destroying cursor position.
  const skipSyncRef = useRef(false);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
      Placeholder.configure({ placeholder: 'Start writing...' }),
      Image.configure({ allowBase64: true, HTMLAttributes: { class: 'write-editor-image' } }),
      Dropcursor,
    ],
    content: value ? markdownToTipTapJson(value) : { type: 'doc', content: [{ type: 'paragraph' }] },
    onUpdate: ({ editor: ed }) => {
      skipSyncRef.current = true;
      const md = tipTapJsonToMarkdown(ed.getJSON());
      onChange(md);
    },
    onSelectionUpdate: ({ editor: ed }) => {
      const { from, to, empty } = ed.state.selection;
      if (empty) {
        useWriteStore.getState().setSelection(null);
        return;
      }
      const text = ed.state.doc.textBetween(from, to);
      if (!text) {
        useWriteStore.getState().setSelection(null);
        return;
      }
      // Compute 0-based line numbers from text offsets
      const docText = ed.state.doc.textBetween(0, ed.state.doc.content.size);
      const lineFrom = docText.slice(0, from).split('\n').length - 1;
      const lineTo = docText.slice(0, to).split('\n').length - 1;
      useWriteStore.getState().setSelection({
        text,
        from,
        to,
        lineFrom,
        lineTo,
        blockType: null,
        containsImage: false,
      });
    },
    editorProps: {
      attributes: {
        style: `font-size: ${fontSize}px; line-height: ${lineHeight}; outline: none;`,
      },
      handlePaste: (_view, event) => {
        if (workspaceRoot && editor) {
          const cd = event.clipboardData;
          if (cd) {
            handleImagePaste(editor, cd, workspaceRoot);
            return true;
          }
        }
        return false;
      },
      handleDrop: (_view, event) => {
        if (workspaceRoot && editor) {
          handleImageDrop(editor, event as unknown as DragEvent, workspaceRoot);
          return true;
        }
        return false;
      },
    },
  });

  // Sync external value changes (e.g., file open, AI apply-edit, mode switch).
  // Skip when the change originated from this editor's own onUpdate so we
  // don't destroy the cursor / selection on every keystroke.
  useEffect(() => {
    if (!editor) return;
    if (skipSyncRef.current) {
      skipSyncRef.current = false;
      return;
    }
    if (value !== undefined) {
      const currentMd = tipTapJsonToMarkdown(editor.getJSON());
      if (currentMd !== value) {
        editor.commands.setContent(markdownToTipTapJson(value));
      }
    }
  }, [value, editor]);

  // Sync font size and line height
  useEffect(() => {
    if (editor) {
      const dom = editor.view.dom as HTMLElement;
      dom.style.fontSize = `${fontSize}px`;
      dom.style.lineHeight = String(lineHeight);
    }
  }, [fontSize, lineHeight, editor]);

  if (!editor) return null;

  // Click handler: open ImageLightbox when clicking an image in the editor
  const handleContainerClick = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement
    if (target.tagName === 'IMG' && target instanceof HTMLImageElement) {
      openLightbox(target.src)
    }
  }, [openLightbox])

  return (
    <div className={styles.editor} onClick={handleContainerClick}>
      <EditorContent editor={editor} />
    </div>
  );
};
