import React, { useEffect } from 'react';
import { useEditor, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import Image from '@tiptap/extension-image';
import Dropcursor from '@tiptap/extension-dropcursor';
import { markdownToTipTapJson, tipTapJsonToMarkdown } from './markdown-projection';
import { handleImagePaste, handleImageDrop } from './paste-image';
import { useWriteStore } from '../../stores/write';

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

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
      Placeholder.configure({ placeholder: 'Start writing...' }),
      Image.configure({ allowBase64: true, HTMLAttributes: { class: 'write-editor-image' } }),
      Dropcursor,
    ],
    content: value ? markdownToTipTapJson(value) : { type: 'doc', content: [{ type: 'paragraph' }] },
    onUpdate: ({ editor: ed }) => {
      const md = tipTapJsonToMarkdown(ed.getJSON());
      onChange(md);
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

  // Sync external value changes (e.g., file open, mode switch)
  useEffect(() => {
    if (editor && value !== undefined) {
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

  return (
    <div
      className="write-rich-editor"
      style={{ flex: 1, overflow: 'auto', padding: '16px 24px' }}
    >
      <EditorContent editor={editor} />
    </div>
  );
};
