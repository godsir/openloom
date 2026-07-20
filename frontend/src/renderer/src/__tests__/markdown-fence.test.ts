import { describe, it, expect } from 'vitest'
import { unclosedFenceStart } from '../utils/markdown-fence'

describe('unclosedFenceStart', () => {
  it('returns -1 when there is no fence', () => {
    expect(unclosedFenceStart('plain text\nmore text')).toBe(-1)
    expect(unclosedFenceStart('')).toBe(-1)
  })

  it('detects an unclosed fence at the start', () => {
    expect(unclosedFenceStart('```python\nprint(1)\nprint(2)')).toBe(0)
    expect(unclosedFenceStart('```\ncode')).toBe(0)
  })

  it('detects an unclosed fence after a closed one', () => {
    const text = 'intro\n```js\ncode\n```\nafter\n```py\nmore'
    // 第二个围栏（未闭合）的起始下标
    expect(unclosedFenceStart(text)).toBe(text.lastIndexOf('```py'))
  })

  it('returns -1 when all fences are closed', () => {
    expect(unclosedFenceStart('```js\ncode\n```')).toBe(-1)
    expect(unclosedFenceStart('a\n```\nb\n```\nc')).toBe(-1)
  })

  it('handles ~~~ fences', () => {
    expect(unclosedFenceStart('~~~\ncode')).toBe(0)
    expect(unclosedFenceStart('~~~\ncode\n~~~')).toBe(-1)
  })

  it('does not treat a shorter closing marker as closing', () => {
    // 开栏 ````(4)，闭栏 ```(3) 不足以闭合
    expect(unclosedFenceStart('````\ncode\n```')).toBe(0)
    // 等长可闭合
    expect(unclosedFenceStart('````\ncode\n````')).toBe(-1)
  })

  it('allows leading indentation on fence lines', () => {
    expect(unclosedFenceStart('  ```js\ncode')).toBe(0)
  })

  it('does not match inline backticks or mid-line fences', () => {
    expect(unclosedFenceStart('use `code` inline')).toBe(-1)
    expect(unclosedFenceStart('text ``` not a fence')).toBe(-1)
  })
})
