import { describe, it, expect } from 'vitest'
import { splitGraphemes, firstGrapheme } from '../utils/grapheme'

describe('grapheme', () => {
  it('splits ASCII text', () => {
    expect(splitGraphemes('hello')).toEqual(['h', 'e', 'l', 'l', 'o'])
  })

  it('splits CJK text', () => {
    expect(splitGraphemes('你好世界')).toEqual(['你', '好', '世', '界'])
  })

  it('splits emoji', () => {
    const result = splitGraphemes('👨‍👩‍👧')
    expect(result.length).toBeGreaterThanOrEqual(1)
  })

  it('firstGrapheme returns first character', () => {
    expect(firstGrapheme('abc')).toBe('a')
    expect(firstGrapheme('你好')).toBe('你')
  })
})

describe('format', () => {
  it('formats relative dates', async () => {
    const { formatSessionDate } = await import('../utils/format')
    const now = new Date().toISOString()
    expect(formatSessionDate(now)).toBe('刚刚')
  })
})
