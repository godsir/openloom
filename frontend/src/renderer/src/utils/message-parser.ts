import type { Message, ContentBlock } from '../stores/chat'

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

// Parse user message content blocks from raw text + attachments
export function parseUserContent(text: string, files?: string[]): ContentBlock[] {
  const blocks: ContentBlock[] = []

  if (text) {
    blocks.push({ type: 'text', html: escapeHtml(text).replace(/\n/g, '<br>'), source: text })
  }

  if (files) {
    for (const file of files) {
      blocks.push({ type: 'file', name: file.split(/[/\\]/).pop() || file, path: file })
    }
  }

  return blocks
}

// Extract tool detail summary for display (file path, URL hostname, etc.)
export function extractToolDetail(name: string, args: Record<string, unknown>): string {
  switch (name) {
    case 'file_read':
    case 'read_file':
      return (args.file_path as string) || (args.path as string) || ''
    case 'file_write':
    case 'write_file':
      return (args.file_path as string) || (args.path as string) || ''
    case 'web_search':
      return (args.query as string) || ''
    case 'web_fetch':
      return (args.url as string) || ''
    case 'execute_command':
    case 'shell':
      return (args.command as string) || ''
    case 'lsp_diagnostics':
    case 'lsp_hover':
    case 'lsp_definition':
    case 'lsp_references':
    case 'lsp_symbols':
      return (args.file_path as string) || ''
    default:
      return ''
  }
}
