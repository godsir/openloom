import { describe, it, expect } from 'vitest'
import { toggleId, allSelected } from '../importSelection'

describe('importSelection helpers', () => {
  it('toggleId adds then removes an id', () => {
    expect(toggleId([], 'a')).toEqual(['a'])
    expect(toggleId(['a'], 'a')).toEqual([])
    expect(toggleId(['a', 'b'], 'a')).toEqual(['b'])
  })

  it('allSelected is true only when every id is selected', () => {
    expect(allSelected(['a', 'b'], ['a', 'b'])).toBe(true)
    expect(allSelected(['a', 'b'], ['a'])).toBe(false)
    expect(allSelected([], ['a'])).toBe(false)
  })
})
