import { describe, expect, it } from 'vitest'
import { sliceByCodePoints } from './text'

describe('bridge/span-roundtrip', () => {
  it('uses Unicode code points rather than UTF-16 offsets', () => {
    expect(sliceByCodePoints('今天☕花了 5 元', 2, 3)).toBe('☕')
    expect(sliceByCodePoints('今天☕花了 5 元', 6, 9)).toBe('5 元')
  })
})
