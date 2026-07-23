import { describe, expect, it, vi } from 'vitest'
import { copyText } from '../clipboard'

describe('copyText', () => {
  it('uses the Electron clipboard bridge so portal documents do not require focus', async () => {
    const writeText = vi.fn(async () => undefined)

    await copyText('copied from a floating window', { writeText })

    expect(writeText).toHaveBeenCalledWith('copied from a floating window')
  })
})
