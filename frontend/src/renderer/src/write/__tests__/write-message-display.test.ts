import { describe, expect, it } from 'vitest'
import { getWriteMessageDisplayText } from '../write-message-display'

describe('getWriteMessageDisplayText', () => {
  it('shows only the user instruction from a composed writing prompt', () => {
    const prompt = [
      '[写作上下文]',
      '当前文件: chapter.md',
      'A very long document',
      '[/写作上下文]',
      '[引用片段]',
      '[引用原文] 来源: chapter.md (行 1-2)',
      'quoted text',
      '[/引用原文]',
      '[/引用片段]',
      '[用户指令]',
      '把这一段润色得更自然',
    ].join('\n')

    expect(getWriteMessageDisplayText(prompt)).toBe('把这一段润色得更自然')
  })

  it('keeps ordinary chat text unchanged', () => {
    expect(getWriteMessageDisplayText('普通消息')).toBe('普通消息')
  })
})
