// TipTap JSONContent ↔ Markdown bidirectional projection
// Markdown is the Source of Truth. All AI operations only understand Markdown.

import type { JSONContent } from '@tiptap/react';

// ============================================================
// TipTap JSON → Markdown
// ============================================================

export function tipTapJsonToMarkdown(doc: JSONContent): string {
  if (!doc.content) return '';
  return doc.content.map(nodeToMarkdown).join('\n\n');
}

function nodeToMarkdown(node: JSONContent): string {
  switch (node.type) {
    case 'paragraph':
      return (node.content ?? []).map(inlineToMarkdown).join('');

    case 'heading': {
      const level = node.attrs?.level ?? 1;
      const prefix = '#'.repeat(level) + ' ';
      return prefix + (node.content ?? []).map(inlineToMarkdown).join('');
    }

    case 'bulletList':
      return (node.content ?? [])
        .map((item) => '- ' + (item.content ?? []).map((p) =>
          (p.content ?? []).map(inlineToMarkdown).join('')
        ).join('\n  '))
        .join('\n');

    case 'orderedList':
      return (node.content ?? [])
        .map((item, i) => `${i + 1}. ` + (item.content ?? []).map((p) =>
          (p.content ?? []).map(inlineToMarkdown).join('')
        ).join('\n   '))
        .join('\n');

    case 'blockquote':
      return (node.content ?? [])
        .map(nodeToMarkdown)
        .join('\n')
        .split('\n')
        .map((l) => '> ' + l)
        .join('\n');

    case 'codeBlock':
      return '```' + (node.attrs?.language ?? '') + '\n' +
        (node.content?.[0]?.text ?? '') + '\n```';

    case 'horizontalRule':
      return '---';

    case 'image':
      return `![${node.attrs?.alt ?? ''}](${node.attrs?.src ?? ''})`;

    case 'listItem':
      return (node.content ?? []).map(nodeToMarkdown).join('');

    default:
      if (node.content) {
        return node.content.map(nodeToMarkdown).join('\n\n');
      }
      return '';
  }
}

function inlineToMarkdown(node: JSONContent): string {
  if (node.type === 'text') {
    let text = node.text ?? '';
    if (node.marks) {
      for (const mark of node.marks) {
        switch (mark.type) {
          case 'bold': text = `**${text}**`; break;
          case 'italic': text = `*${text}*`; break;
          case 'strike': text = `~~${text}~~`; break;
          case 'code': text = `\`${text}\``; break;
          case 'link':
            text = `[${text}](${mark.attrs?.href ?? ''})`;
            break;
        }
      }
    }
    return text;
  }
  if (node.type === 'hardBreak') return '\n';
  if (node.type === 'image') return `![${node.attrs?.alt ?? ''}](${node.attrs?.src ?? ''})`;
  return '';
}

// ============================================================
// Markdown → TipTap JSON
// ============================================================

export function markdownToTipTapJson(markdown: string): JSONContent {
  const lines = markdown.split('\n');
  const content: JSONContent[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Empty line
    if (line.trim() === '') { i++; continue; }

    // Heading
    const headingMatch = line.match(/^(#{1,6})\s+(.+)/);
    if (headingMatch) {
      content.push({
        type: 'heading',
        attrs: { level: headingMatch[1].length },
        content: parseInlineContent(headingMatch[2]),
      });
      i++; continue;
    }

    // Code block
    if (line.startsWith('```')) {
      const lang = line.slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith('```')) {
        codeLines.push(lines[i]);
        i++;
      }
      i++; // skip closing ```
      content.push({
        type: 'codeBlock',
        attrs: { language: lang || null },
        content: codeLines.length > 0
          ? [{ type: 'text', text: codeLines.join('\n') }]
          : undefined,
      });
      continue;
    }

    // Unordered list
    if (line.match(/^[-*+]\s+/)) {
      const listItems: JSONContent[] = [];
      while (i < lines.length && lines[i].match(/^[-*+]\s+/)) {
        listItems.push({
          type: 'listItem',
          content: [{
            type: 'paragraph',
            content: parseInlineContent(lines[i].replace(/^[-*+]\s+/, '')),
          }],
        });
        i++;
      }
      content.push({ type: 'bulletList', content: listItems });
      continue;
    }

    // Ordered list
    if (line.match(/^\d+\.\s+/)) {
      const listItems: JSONContent[] = [];
      while (i < lines.length && lines[i].match(/^\d+\.\s+/)) {
        listItems.push({
          type: 'listItem',
          content: [{
            type: 'paragraph',
            content: parseInlineContent(lines[i].replace(/^\d+\.\s+/, '')),
          }],
        });
        i++;
      }
      content.push({ type: 'orderedList', content: listItems });
      continue;
    }

    // Blockquote
    if (line.startsWith('> ')) {
      const quoteLines: string[] = [];
      while (i < lines.length && lines[i].startsWith('> ')) {
        quoteLines.push(lines[i].slice(2));
        i++;
      }
      content.push({
        type: 'blockquote',
        content: [{
          type: 'paragraph',
          content: parseInlineContent(quoteLines.join('\n')),
        }],
      });
      continue;
    }

    // Horizontal rule
    if (line.match(/^[-*_]{3,}$/)) {
      content.push({ type: 'horizontalRule' });
      i++; continue;
    }

    // Default paragraph
    content.push({
      type: 'paragraph',
      content: parseInlineContent(line),
    });
    i++;
  }

  if (content.length === 0) {
    content.push({ type: 'paragraph' });
  }

  return { type: 'doc', content };
}

export function parseInlineContent(text: string): JSONContent[] {
  const nodes: JSONContent[] = [];
  const regex = /(\*\*(.+?)\*\*|(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)|~~(.+?)~~|`(.+?)`|!\[([^\]]*)\]\(([^)]+)\)|\[([^\]]+)\]\(([^)]+)\)|[^*~`!\[\]]+)/g;
  let match;

  while ((match = regex.exec(text)) !== null) {
    if (match[2] !== undefined) {
      // **bold**
      nodes.push({ type: 'text', text: match[2], marks: [{ type: 'bold' }] });
    } else if (match[3] !== undefined) {
      // *italic*
      nodes.push({ type: 'text', text: match[3], marks: [{ type: 'italic' }] });
    } else if (match[4] !== undefined) {
      // ~~strikethrough~~
      nodes.push({ type: 'text', text: match[4], marks: [{ type: 'strike' }] });
    } else if (match[5] !== undefined) {
      // `code`
      nodes.push({ type: 'text', text: match[5], marks: [{ type: 'code' }] });
    } else if (match[6] !== undefined && match[7] !== undefined) {
      // ![alt](src)
      nodes.push({ type: 'image', attrs: { alt: match[6], src: match[7] } });
    } else if (match[8] !== undefined && match[9] !== undefined) {
      // [text](href)
      nodes.push({ type: 'text', text: match[8], marks: [{ type: 'link', attrs: { href: match[9] } }] });
    } else {
      // plain text
      nodes.push({ type: 'text', text: match[0] });
    }
  }

  if (nodes.length === 0) {
    nodes.push({ type: 'text', text });
  }

  return nodes;
}
