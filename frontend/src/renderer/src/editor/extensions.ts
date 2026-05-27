// TipTap editor extensions: SkillBadge + FileBadge.
// Custom inline nodes rendered as chips in the editor.

import { Node } from '@tiptap/core'

export const SkillBadge = Node.create({
  name: 'skillBadge',
  group: 'inline',
  inline: true,
  atom: true,

  addAttributes() {
    return {
      name: { default: '' },
    }
  },

  parseHTML() {
    return [{ tag: 'span[data-skill]' }]
  },

  renderHTML({ HTMLAttributes }) {
    return [
      'span',
      {
        'data-skill': HTMLAttributes.name,
        class: 'inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-blue-900/50 text-blue-300 text-xs font-mono',
      },
      `@${HTMLAttributes.name}`,
    ]
  },
})

export const FileBadge = Node.create({
  name: 'fileBadge',
  group: 'inline',
  inline: true,
  atom: true,

  addAttributes() {
    return {
      path: { default: '' },
      name: { default: '' },
    }
  },

  parseHTML() {
    return [{ tag: 'span[data-file]' }]
  },

  renderHTML({ HTMLAttributes }) {
    return [
      'span',
      {
        'data-file': HTMLAttributes.path,
        class: 'inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-zinc-700 text-zinc-300 text-xs',
      },
      `📎 ${HTMLAttributes.name || HTMLAttributes.path}`,
    ]
  },
})
