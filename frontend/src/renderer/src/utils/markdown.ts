// Markdown rendering pipeline — to be wired fully in Task 4.3.
// For now, provides the basic markdown-it renderer.
import MarkdownIt from 'markdown-it'
import katex from 'katex'

const md = new MarkdownIt({
  html: false,
  breaks: true,
  linkify: true,
  typographer: true,
})

// Obsidian callout support
const CALLOUT_RE = /^\[!(\w+)\]([+-]?)\s*(.*)$/

function calloutPlugin(md: MarkdownIt) {
  const defaultRender =
    md.renderer.rules.blockquote_open ||
    function (tokens, idx, options, _env, self) {
      return self.renderToken(tokens, idx, options)
    }

  md.renderer.rules.blockquote_open = function (tokens, idx, options, env, self) {
    return defaultRender(tokens, idx, options, env, self)
  }
}

// KaTeX math support: \( \) and \[ \]
function katexPlugin(md: MarkdownIt) {
  const defaultText = md.renderer.rules.text || function (tokens, idx) {
    return md.utils.escapeHtml(tokens[idx].content)
  }

  md.renderer.rules.text = function (tokens, idx, options, env, self) {
    const content = tokens[idx].content
    // Inline math: \( ... \)
    if (content.startsWith('\\(') && content.endsWith('\\)')) {
      try {
        const math = content.slice(2, -2)
        return katex.renderToString(math, { throwOnError: false, displayMode: false })
      } catch {
        return defaultText(tokens, idx, options, env, self)
      }
    }
    return defaultText(tokens, idx, options, env, self)
  }
}

md.use(calloutPlugin)
md.use(katexPlugin)

export function renderMarkdown(text: string): string {
  return md.render(text)
}

export { md }
