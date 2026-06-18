/**
 * Dynamic theme color reading — reads actual CSS custom property values
 * by temporarily applying a theme to a hidden DOM element.
 *
 * This avoids maintaining duplicate hardcoded color values that drift
 * out of sync with the theme CSS files.
 */

const cache = new Map<string, ThemeColors>()

export interface ThemeColors {
  bg: string
  surface: string
  text: string
  accent: string
  /** text color at ~13% opacity — for subtle preview bars */
  text13: string
  /** text color at ~27% opacity — for slightly more visible preview bars */
  text27: string
}

function parseRgb(color: string): [number, number, number] | null {
  const m = color.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/)
  if (!m) return null
  return [parseInt(m[1]), parseInt(m[2]), parseInt(m[3])]
}

/**
 * Read CSS custom properties (--bg, --bg-surface, --text, --accent)
 * for the given theme ID. Results are cached — theme CSS doesn't change at runtime.
 * Returns null if the theme has no CSS rules (e.g. 'custom').
 */
export function readThemeColors(themeId: string): ThemeColors | null {
  if (cache.has(themeId)) return cache.get(themeId)!

  const el = document.createElement('div')
  el.style.display = 'none'
  el.setAttribute('data-theme', themeId)
  document.body.appendChild(el)

  const s = getComputedStyle(el)
  const bg = s.getPropertyValue('--bg').trim()
  const surface = s.getPropertyValue('--bg-surface').trim()
  const text = s.getPropertyValue('--text').trim()
  const accent = s.getPropertyValue('--accent').trim()

  document.body.removeChild(el)

  if (!bg || !accent) return null

  // Convert text color (e.g. rgba(15,23,42,0.92)) to low-opacity variants
  // for the preview bars in the theme selector card.
  const rgb = parseRgb(text)
  const text13 = rgb ? `rgba(${rgb[0]},${rgb[1]},${rgb[2]},0.13)` : text
  const text27 = rgb ? `rgba(${rgb[0]},${rgb[1]},${rgb[2]},0.27)` : text

  const colors: ThemeColors = { bg, surface, text, accent, text13, text27 }
  cache.set(themeId, colors)
  return colors
}
