import React, { useEffect, useRef, useCallback, forwardRef, useImperativeHandle } from 'react';
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
import type { WriteBlockType } from '../block-type';
import type { InlineFormatKind } from '../inline-format';

interface WriteRichEditorProps {
  value: string;
  onChange: (markdown: string) => void;
  fontSize?: number;
}

export interface RichEditorActiveState {
  block: WriteBlockType | null;
  bold: boolean;
  italic: boolean;
  strikethrough: boolean;
  code: boolean;
}

export interface WriteRichEditorHandle {
  applyBlock: (type: WriteBlockType) => boolean;
  toggleInline: (kind: InlineFormatKind) => boolean;
  getActiveState: () => RichEditorActiveState;
  undo: () => boolean;
  redo: () => boolean;
}

export const WriteRichEditor = forwardRef<WriteRichEditorHandle, WriteRichEditorProps>(function WriteRichEditor({
  value,
  onChange,
  fontSize = 14,
}, ref) {
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
      // from/to 是 ProseMirror 位置（含节点边界计数），不能直接当作扁平文本偏移去
      // slice（旧实现导致行号恒为 0）。用 textBetween 的 blockSeparator 把"from 之前
      // 的文本"一致地取出来再数换行，from/to 同口径，行号才有意义（A2）。
      const textBeforeFrom = ed.state.doc.textBetween(0, from, '\n\n', '\n');
      const textBeforeTo = ed.state.doc.textBetween(0, to, '\n\n', '\n');
      const lineFrom = textBeforeFrom.split('\n').length - 1;
      const lineTo = textBeforeTo.split('\n').length - 1;
      useWriteStore.getState().setSelection({
        source: 'rich',
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
          const hasImage = cd ? Array.from(cd.items).some((item) => item.type.startsWith('image/')) : false;
          if (cd && hasImage) {
            void handleImagePaste(editor, cd, workspaceRoot);
            return true;
          }
        }
        return false;
      },
      handleDrop: (_view, event) => {
        if (workspaceRoot && editor) {
          const dragEvent = event as unknown as DragEvent;
          const hasImage = Array.from(dragEvent.dataTransfer?.files ?? []).some((file) => file.type.startsWith('image/'));
          if (hasImage) {
            void handleImageDrop(editor, dragEvent, workspaceRoot);
            return true;
          }
        }
        return false;
      },
    },
  });

  useImperativeHandle(ref, () => ({
    applyBlock: (type) => {
      if (!editor) return false;
      const chain = editor.chain().focus();
      switch (type) {
        case 'paragraph': return chain.setParagraph().run();
        case 'heading1': return chain.setHeading({ level: 1 }).run();
        case 'heading2': return chain.setHeading({ level: 2 }).run();
        case 'heading3': return chain.setHeading({ level: 3 }).run();
        case 'quote': return chain.toggleBlockquote().run();
        case 'bullet': return chain.toggleBulletList().run();
        case 'ordered': return chain.toggleOrderedList().run();
        case 'code': return chain.toggleCodeBlock().run();
      }
    },
    toggleInline: (kind) => {
      if (!editor) return false;
      const chain = editor.chain().focus();
      switch (kind) {
        case 'bold': return chain.toggleBold().run();
        case 'italic': return chain.toggleItalic().run();
        case 'strikethrough': return chain.toggleStrike().run();
        case 'code': return chain.toggleCode().run();
      }
    },
    getActiveState: () => {
      if (!editor) return { block: null, bold: false, italic: false, strikethrough: false, code: false };
      const block: WriteBlockType =
        editor.isActive('heading', { level: 1 }) ? 'heading1' :
        editor.isActive('heading', { level: 2 }) ? 'heading2' :
        editor.isActive('heading', { level: 3 }) ? 'heading3' :
        editor.isActive('blockquote') ? 'quote' :
        editor.isActive('bulletList') ? 'bullet' :
        editor.isActive('orderedList') ? 'ordered' :
        editor.isActive('codeBlock') ? 'code' : 'paragraph';
      return {
        block,
        bold: editor.isActive('bold'),
        italic: editor.isActive('italic'),
        strikethrough: editor.isActive('strike'),
        code: editor.isActive('code'),
      };
    },
    undo: () => editor?.chain().focus().undo().run() ?? false,
    redo: () => editor?.chain().focus().redo().run() ?? false,
  }), [editor]);

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
        editor.commands.setContent(markdownToTipTapJson(value), { emitUpdate: false });
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

  useEffect(() => () => {
    useWriteStore.getState().setSelection(null);
  }, []);

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
});
