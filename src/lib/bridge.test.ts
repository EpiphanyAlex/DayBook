import { describe, expect, it } from 'vitest'
import { AppError, parseIpcIntegers, serializeIpcIntegers } from './bridge'

function expectAppErrorCode(operation: () => unknown, code: string) {
  try {
    operation()
    throw new Error('预期抛出 AppError')
  } catch (error: unknown) {
    expect(error).toBeInstanceOf(AppError)
    expect((error as AppError).code).toBe(code)
  }
}

describe('bridge/money-parse', () => {
  it('parses money strings only at the IPC boundary', () => {
    expect(parseIpcIntegers({ amountMinor: '168', merchant: '咖啡' })).toEqual({
      amountMinor: 168,
      merchant: '咖啡',
    })
  })

  it('rejects out-of-range values', () => {
    expectAppErrorCode(
      () => parseIpcIntegers({ amountMinor: '1000000000000001' }),
      'data.amount_out_of_range',
    )
  })

  it('rejects JSON numbers for money fields', () => {
    expectAppErrorCode(() => parseIpcIntegers({ amountMinor: 168 }), 'data.invalid_argument')
  })

  it('serializes safe integer inputs as decimal strings', () => {
    expect(serializeIpcIntegers({ amountMinor: 168, ratePpm: 1_000_000 })).toEqual({
      amountMinor: '168',
      ratePpm: '1000000',
    })
  })
})
