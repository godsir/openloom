import { describe, expect, it, vi } from 'vitest'
import { openWriteAssistantWindow } from '../write-assistant-window'

describe('openWriteAssistantWindow', () => {
  it('opens the named resizable assistant window synchronously', () => {
    const popup = {} as Window
    const open = vi.fn(() => popup)

    expect(openWriteAssistantWindow(open)).toBe(popup)
    expect(open).toHaveBeenCalledWith(
      'about:blank',
      'openloom-write-assistant',
      'popup=yes,width=380,height=640,resizable=yes',
    )
  })
})
