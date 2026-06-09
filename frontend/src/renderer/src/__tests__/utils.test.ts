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
    const t = (key: string, vars?: Record<string, string | number>) => {
      const map: Record<string, string> = {
        'time.justNow': '刚刚',
        'time.minutesAgo': '{n} 分钟前',
        'time.hoursAgo': '{n} 小时前',
        'time.daysAgo': '{n} 天前',
      }
      let text = map[key] || key
      if (vars) {
        text = text.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`))
      }
      return text
    }
    const now = new Date().toISOString()
    expect(formatSessionDate(now, t)).toBe('刚刚')
  })
})
