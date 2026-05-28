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
        class: 'inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-[var(--bg-card)] text-[var(--t-secondary)] text-xs border border-[var(--b-subtle)]',
      },
import { IconPaperclip } from '../../utils/icons'

// ... later in renderHTML ...
      `<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="display:inline;vertical-align:middle;margin-right:2px"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>${HTMLAttributes.name || HTMLAttributes.path}`,
    ]
  },
})
