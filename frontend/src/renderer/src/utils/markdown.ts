// Markdown rendering pipeline — to be wired fully in Task 4.3.
// For now, provides the basic markdown-it renderer.
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js'
import katex from 'katex'
import { t } from '../i18n'

const md = new MarkdownIt({
  html: true,
  breaks: true,
  linkify: true,
  typographer: true,
})

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
        `<button class="open-file-btn" data-file-path="${escaped}" title="${t('common.openFile')}">${t('common.open')}</button></span>`
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

// File path pattern for extracting file paths from fence info strings
const FILE_PATH_RE = /([A-Za-z]:\\(?:\S+?\\)*\S+\.\w{1,10}|\/(?:\S+\/)+\S+\.\w{1,10})/

// Mermaid diagram support — outputs a placeholder div that TextBlock's
// useEffect replaces with a rendered SVG via the lazy-loaded mermaid library.
function mermaidPlugin(md: MarkdownIt) {
  const defaultFence = md.renderer.rules.fence || function (tokens, idx, options, env, self) {
    return self.renderToken(tokens, idx, options)
  }

  md.renderer.rules.fence = function (tokens, idx, options, env, self) {
    const token = tokens[idx]
    const info = token.info.trim()
    if (info === 'mermaid' || info.startsWith('mermaid ')) {
      const escaped = md.utils.escapeHtml(token.content)
      return `<div class="mermaid-placeholder" data-mermaid-source="${escaped}"><pre><code class="language-mermaid">${escaped}</code></pre></div>`
    }

    // Extract file path from fence info (e.g. "typescript /path/to/file.ts" or "python D:\code\file.py")
    const fpMatch = info.match(FILE_PATH_RE)
    const filePath = fpMatch ? fpMatch[1] : ''
    const lang = fpMatch ? info.replace(fpMatch[0], '').trim() : info
    const code = token.content
    const dataAttrs = filePath
      ? ` data-file-path="${md.utils.escapeHtml(filePath)}"`
      : ''

    // Syntax highlighting via highlight.js; falls back to escaped plain text
    // when the language is unknown or absent.
    let highlighted: string
    if (lang && hljs.getLanguage(lang)) {
      highlighted = hljs.highlight(code, { language: lang }).value
    } else {
      highlighted = md.utils.escapeHtml(code)
    }

    const langLabel = lang ? md.utils.escapeHtml(lang) : ''
    return `<div class="code-block-wrapper"${dataAttrs}><div class="code-block-header"><span class="code-block-lang">${langLabel}</span><button class="copy-code-btn" type="button">${t('common.copy', '复制')}</button></div><pre><code class="hljs${lang ? ` language-${md.utils.escapeHtml(lang)}` : ''}">${highlighted}</code></pre></div>`
  }
}

md.use(katexPlugin)
md.use(filePathPlugin)
md.use(mermaidPlugin)

export function renderMarkdown(text: string): string {
  return md.render(text)
}

export { md }
