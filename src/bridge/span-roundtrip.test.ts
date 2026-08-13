import { describe, expect, it } from 'vitest'
import { sliceByCodePoints } from '../lib/text'

describe('bridge/span-roundtrip acceptance selector', () => {
  it('indexes emoji and Chinese text by Unicode code point', () => {
    expect(sliceByCodePoints('A😀中B', 1, 3)).toBe('😀中')
  })
})
