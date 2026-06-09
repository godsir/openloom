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
  'class', 'id', 'data-theme', 'data-choice', 'data-stream-tail-char', 'data-file-path',
  'checked', 'disabled', 'type',
])

const BLOCKED_SRC_RE = /^https?:\/\//i

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

      // Strip javascript: URLs
      const href = el.getAttribute('href') || ''
      if (href.toLowerCase().startsWith('javascript:')) {
        el.removeAttribute('href')
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
