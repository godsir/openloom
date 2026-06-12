// DOM-based HTML sanitizer for markdown output.
// Strips remote images and dangerous tags while preserving safe formatting.

import { t } from '../i18n'

const ALLOWED_TAGS = new Set([
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'p', 'br', 'hr',
  'strong', 'em', 'del', 'ins', 'mark', 'sup', 'sub',
  'a', 'img',
  'ul', 'ol', 'li',
  'pre', 'code',
  'blockquote',
  'table', 'thead', 'tbody', 'tr', 'th', 'td',
  'span', 'div',
  'input', // for task checkboxes
  'button', // for file-path open buttons
])

const ALLOWED_ATTRS = new Set([
  'href', 'src', 'alt', 'title', 'width', 'height',
  'class', 'data-theme', 'data-choice', 'data-stream-tail-char', 'data-file-path',
  'checked', 'disabled', 'type',
])

const BLOCKED_SRC_RE = /^https?:\/\//i

// Resolve a URL's scheme by stripping leading control chars/whitespace that
// browsers tolerate inside URLs (e.g. "java\tscript:"). Returns the lowercased
// scheme (without the trailing ':') or '' if the value has no explicit scheme.
function urlScheme(value: string): string {
  const cleaned = value.replace(/[\u0000- ]+/g, '')
  const match = /^([a-z][a-z0-9+.-]*):/i.exec(cleaned)
  return match ? match[1].toLowerCase() : ''
}

// href may only navigate to these schemes (or be scheme-relative / relative).
const ALLOWED_HREF_SCHEMES = new Set(['http', 'https', 'mailto'])

export function sanitizeHtml(html: string): string {
  const doc = new DOMParser().parseFromString(html, 'text/html')

  function clean(node: Node): void {
    if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node as HTMLElement

      // Remove disallowed elements
      if (!ALLOWED_TAGS.has(el.tagName.toLowerCase())) {
        while (el.firstChild) {
          el.parentNode?.insertBefore(el.firstChild, el)
        }
        el.parentNode?.removeChild(el)
        return
      }

      // Strip disallowed attributes
      for (const attr of [...el.attributes]) {
        if (!ALLOWED_ATTRS.has(attr.name)) {
          el.removeAttribute(attr.name)
        }
      }

      // Validate src scheme: allow http(s) and data:image/ only; strip anything
      // else (javascript:, data:text/html, etc.). Remote http(s) images are
      // additionally blocked below for privacy.
      if (el.hasAttribute('src')) {
        const src = el.getAttribute('src') || ''
        const scheme = urlScheme(src)
        const isDataImage = /^data:image\//i.test(src.trimStart())
        if (scheme && !isDataImage && scheme !== 'http' && scheme !== 'https') {
          el.removeAttribute('src')
        }
      }

      // Block remote image sources (privacy) — keep data: URLs intact
      if (el.tagName === 'IMG') {
        const src = el.getAttribute('src') || ''
        if (BLOCKED_SRC_RE.test(src)) {
          el.setAttribute('data-blocked-src', src)
          el.removeAttribute('src')
          el.setAttribute('title', t('common.clickToLoadImage'))
          el.classList.add('blocked-image')
        }
      }

      // Restrict href to a safe scheme allowlist (http/https/mailto). Relative
      // and scheme-relative URLs (no explicit scheme) are permitted.
      if (el.hasAttribute('href')) {
        const href = el.getAttribute('href') || ''
        const scheme = urlScheme(href)
        if (scheme && !ALLOWED_HREF_SCHEMES.has(scheme)) {
          el.removeAttribute('href')
        }
      }
    }

    // Recurse children
    const children = [...node.childNodes]
    for (const child of children) {
      clean(child)
    }
  }

  if (!doc.body) return html
  // Clean only body's children — never body/html/head themselves,
  // otherwise clean() removes them (not in ALLOWED_TAGS) and doc.body becomes null.
  for (const child of [...doc.body.childNodes]) {
    clean(child)
  }
  return doc.body.innerHTML
}
