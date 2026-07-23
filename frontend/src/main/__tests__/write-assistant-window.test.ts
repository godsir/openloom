import { describe, expect, it } from 'vitest'
import { isSafeExternalWindowUrl, isWriteAssistantWindowRequest } from '../write-assistant-window'

describe('isWriteAssistantWindowRequest', () => {
  it('allows only the named internal writing assistant popup', () => {
    expect(isWriteAssistantWindowRequest('about:blank', 'openloom-write-assistant')).toBe(true)
    expect(isWriteAssistantWindowRequest('about:blank', 'other-window')).toBe(false)
    expect(isWriteAssistantWindowRequest('https://example.com', 'openloom-write-assistant')).toBe(false)
  })
})

describe('isSafeExternalWindowUrl', () => {
  it('allows user-facing external protocols and rejects internal or executable URLs', () => {
    expect(isSafeExternalWindowUrl('https://openloom.dev')).toBe(true)
    expect(isSafeExternalWindowUrl('http://localhost:5173/docs')).toBe(true)
    expect(isSafeExternalWindowUrl('mailto:hello@openloom.dev')).toBe(true)
    expect(isSafeExternalWindowUrl('about:blank')).toBe(false)
    expect(isSafeExternalWindowUrl('file:///C:/secret.txt')).toBe(false)
    expect(isSafeExternalWindowUrl('javascript:alert(1)')).toBe(false)
  })
})
