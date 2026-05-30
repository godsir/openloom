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

// File path detection — wraps absolute paths with an "open" button
function filePathPlugin(md: MarkdownIt) {
  const prevText = md.renderer.rules.text || function (tokens, idx) {
    return md.utils.escapeHtml(tokens[idx].content)
  }
  const prevCode = md.renderer.rules.code_inline || function (tokens, idx) {
    return '<code>' + md.utils.escapeHtml(tokens[idx].content) + '</code>'
  }

  // Match Windows (D:\...) and Unix (/...) absolute paths with a file extension
  const RE = /[A-Za-z]:\\(?:\S+?\\)*\S+\.\w{1,10}|\/(?:\S+\/)+\S+\.\w{1,10}/g

  function wrapPaths(content: string): string {
    RE.lastIndex = 0
    let result = ''
    let lastIndex = 0
    let match: RegExpExecArray | null
    while ((match = RE.exec(content)) !== null) {
      result += md.utils.escapeHtml(content.slice(lastIndex, match.index))
      const path = match[0]
      const escaped = md.utils.escapeHtml(path)
      result += `<span class="file-path-wrapper"><code>${escaped}</code>` +
        `<button class="open-file-btn" data-file-path="${escaped}" title="打开文件">打开</button></span>`
      lastIndex = match.index + path.length
    }
    result += md.utils.escapeHtml(content.slice(lastIndex))
    return result
  }

  md.renderer.rules.text = function (tokens, idx, options, env, self) {
    const content = tokens[idx].content

    // Delegate KaTeX math to the previous renderer (katexPlugin)
    if (content.startsWith('\\(') && content.endsWith('\\)')) {
      return prevText(tokens, idx, options, env, self)
    }

    RE.lastIndex = 0
    if (!RE.test(content)) {
      return prevText(tokens, idx, options, env, self)
    }
    return wrapPaths(content)
  }

  md.renderer.rules.code_inline = function (tokens, idx, options, env, self) {
    const content = tokens[idx].content
    RE.lastIndex = 0
    if (!RE.test(content)) {
      return prevCode(tokens, idx, options, env, self)
    }
    return wrapPaths(content)
  }
}

md.use(calloutPlugin)
md.use(katexPlugin)
md.use(filePathPlugin)

export function renderMarkdown(text: string): string {
  return md.render(text)
}

export { md }
