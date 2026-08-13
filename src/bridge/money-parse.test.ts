import { describe, expect, it } from 'vitest'
import { AppError, parseIpcIntegers } from '../lib/bridge'

describe('bridge/money-parse acceptance selector', () => {
  it('round-trips the supported boundary and rejects values above it', () => {
    expect(parseIpcIntegers({ amountMinor: '1000000000000000' })).toEqual({
      amountMinor: 1_000_000_000_000_000,
    })
    try {
      parseIpcIntegers({ amountMinor: '1000000000000001' })
      throw new Error('expected AppError')
    } catch (error) {
      expect(error).toBeInstanceOf(AppError)
      expect((error as AppError).code).toBe('data.amount_out_of_range')
    }
  })
})
