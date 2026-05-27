// Grapheme-safe text splitting using Intl.Segmenter with fallback.
export function splitGraphemes(text: string): string[] {
  if (typeof Intl !== 'undefined' && Intl.Segmenter) {
    const segmenter = new Intl.Segmenter('en', { granularity: 'grapheme' })
    return [...segmenter.segment(text)].map((s) => s.segment)
  }
  return Array.from(text)
}

export function firstGrapheme(text: string): string {
  const segments = splitGraphemes(text)
  return segments[0] || ''
}

export function displayInitial(text: string): string {
  return firstGrapheme(text)
}
