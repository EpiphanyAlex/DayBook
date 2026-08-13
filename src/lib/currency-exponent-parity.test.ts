import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { AppError } from './bridge'
import { CURRENCY_CODES, currencyExponent, formatMoney } from './money'

/**
 * 两张手抄的 exponent 表一定会分叉，问题只是什么时候。
 * M0 就分叉过：Rust 的 0 位列表里有 UYI，前端那张漏了 —— 于是 9700 UYI 会显示成
 * 97.00 UYI，正是 `.claude/rules/money-and-data.md` §1.1 举的那个 bug 形态。
 * 这条测试直接读 Rust 源里的那张表，逐条比对。
 */
function rustExponents(): Map<string, number> {
  // jsdom 环境下 import.meta.url 不是 file:// —— vitest 的 cwd 就是仓库根。
  const source = readFileSync(resolve(process.cwd(), 'src-tauri/src/money.rs'), 'utf8')
  const start = source.indexOf('let exponent = match code {')
  const end = source.indexOf('_ =>', start)
  expect(start, 'currency_exponent 的 match 块没找到——Rust 侧结构变了').toBeGreaterThan(-1)
  const table = new Map<string, number>()
  for (const [, codes, exponent] of source
    .slice(start, end)
    .matchAll(/((?:\s*\|?\s*"[A-Z]{3}")+)\s*=>\s*(\d)/g)) {
    for (const [, code] of codes.matchAll(/"([A-Z]{3})"/g)) table.set(code, Number(exponent))
  }
  return table
}

describe('lib/currency-exponent-parity', () => {
  const rust = rustExponents()

  it('reads a non-trivial table out of the Rust source', () => {
    expect(rust.size).toBeGreaterThan(100)
    expect(rust.get('JPY')).toBe(0)
    expect(rust.get('KWD')).toBe(3)
  })

  it('covers exactly the same currency codes as Rust', () => {
    expect(CURRENCY_CODES).toEqual([...rust.keys()].sort())
  })

  it('agrees with Rust on every exponent', () => {
    for (const [code, exponent] of rust) expect(currencyExponent(code), code).toBe(exponent)
  })

  it('rejects unknown currencies instead of falling back to 2', () => {
    expect(() => currencyExponent('ZZZ')).toThrowError(AppError)
    try {
      currencyExponent('ZZZ')
    } catch (error) {
      expect((error as AppError).code).toBe('data.unsupported_currency')
    }
  })

  it('formats zero-exponent currencies without inventing decimals', () => {
    expect(formatMoney(9700, 'UYI')).toBe('9700 UYI')
    expect(formatMoney(9700, 'JPY')).toBe('9700 JPY')
    expect(formatMoney(9700, 'AUD')).toBe('97.00 AUD')
    expect(formatMoney(9700, 'KWD')).toBe('9.700 KWD')
  })
})
