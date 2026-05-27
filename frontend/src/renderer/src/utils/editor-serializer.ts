// TipTap editor → backend message bridge.
// Serializes TipTap JSON content into plain text + extracted skill/file badges.

interface TipTapNode {
  type: string
  text?: string
  content?: TipTapNode[]
  attrs?: Record<string, string>
}

export function serializeEditorContent(doc: TipTapNode): {
  text: string
  skills: string[]
  fileRefs: string[]
} {
  const textParts: string[] = []
  const skills: string[] = []
  const fileRefs: string[] = []

  function walk(node: TipTapNode): void {
    if (node.type === 'text' && node.text) {
      textParts.push(node.text)
    }
    if (node.type === 'skillBadge' && node.attrs?.name) {
      skills.push(node.attrs.name)
    }
    if (node.type === 'fileBadge' && node.attrs?.path) {
      fileRefs.push(node.attrs.path)
    }
    if (node.content) {
      for (const child of node.content) walk(child)
    }
  }

  walk(doc)
  return { text: textParts.join(''), skills, fileRefs }
}
