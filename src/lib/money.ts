import { AppError, type MinorUnits, type RatePpm } from './bridge'

/**
 * ISO 4217 minor unit exponent。
 *
 * **这张表是 `src-tauri/src/money.rs::currency_exponent` 的镜像**，由
 * `src/lib/currency-exponent-parity.test.ts` 逐条断言两边一致 —— 手抄的第二张表会分叉，
 * M0 就分叉过一次（Rust 有 UYI、前端漏了，于是 9700 UYI 会显示成 97.00 UYI）。
 * 改动任一侧都必须同时改另一侧，否则那条测试变红。
 */
const EXPONENTS: Record<string, number> = {}

const setAll = (codes: string, exponent: number) => {
  for (const code of codes.split(' ')) EXPONENTS[code] = exponent
}

setAll('BIF CLP DJF GNF ISK JPY KMF KRW PYG RWF UGX UYI VND VUV XAF XOF XPF', 0)
setAll('BHD IQD JOD KWD LYD OMR TND', 3)
setAll('CLF UYW', 4)
setAll(
  'AED AFN ALL AMD ANG AOA ARS AUD AWG AZN BAM BBD BDT BGN BMD BND BOB BOV BRL BSD BTN ' +
    'BWP BYN BZD CAD CDF CHE CHF CHW CNY COP COU CRC CUP CVE CZK DKK DOP DZD EGP ERN ETB ' +
    'EUR FJD FKP GBP GEL GHS GIP GMD GTQ GYD HKD HNL HTG HUF IDR ILS INR IRR JMD KES KGS ' +
    'KHR KPW KYD KZT LAK LBP LKR LRD LSL MAD MDL MGA MKD MMK MNT MOP MRU MUR MVR MWK MXN ' +
    'MXV MYR MZN NAD NGN NIO NOK NPR NZD PAB PEN PGK PHP PKR PLN QAR RON RSD RUB SAR SBD ' +
    'SCR SDG SEK SGD SHP SLE SOS SRD SSP STN SVC SYP SZL THB TJS TMT TOP TRY TTD TWD TZS ' +
    'UAH USD USN UYU UZS VED VES WST XCD YER ZAR ZMW ZWG',
  2,
)

export const CURRENCY_CODES = Object.keys(EXPONENTS).sort()

/**
 * 未知币种是错误，不是「回退到 2」（`.claude/rules/money-and-data.md` §1.1）。
 * 回退到 2 会让一条带错 exponent 的金额一路过掉总额校验，被用户一眼扫过去确认入库。
 */
export function currencyExponent(currency: string): number {
  const exponent = EXPONENTS[currency]
  if (exponent === undefined) {
    throw new AppError({
      code: 'data.unsupported_currency',
      message: `不支持的 ISO 4217 币种：${currency}`,
      detail: null,
    })
  }
  return exponent
}

/** 全前端唯一一处金额除法，且除数由币种决定 —— 这里用字符串切片，连除法都不需要。 */
export function formatMoney(amount: MinorUnits | number | null, currency: string | null): string {
  if (amount === null || currency === null) return '—'
  const exponent = currencyExponent(currency)
  const sign = amount < 0 ? '−' : ''
  const absolute = Math.abs(amount).toString().padStart(exponent + 1, '0')
  const value =
    exponent === 0 ? absolute : `${absolute.slice(0, -exponent)}.${absolute.slice(-exponent)}`
  return `${sign}${value} ${currency}`
}

/** 把用户输入的主单位小数转成最小单位整数；不合法返回 null，由调用方决定怎么提示。 */
export function parseMoneyInput(value: string, currency: string): number | null {
  const normalized = value.trim().replace(/,/g, '')
  if (!/^-?\d+(?:\.\d+)?$/.test(normalized)) return null
  const exponent = currencyExponent(currency)
  const [whole, fraction = ''] = normalized.split('.')
  if (fraction.length > exponent) return null
  const negative = whole.startsWith('-')
  const digits = `${whole.replace('-', '')}${fraction.padEnd(exponent, '0')}`
  const result = Number(digits) * (negative ? -1 : 1)
  return Number.isSafeInteger(result) ? result : null
}

/** `rate_ppm` 是「1 主单位原币 = 多少主单位本位币」× 1_000_000。 */
export function formatRate(ratePpm: RatePpm | number | null): string {
  if (ratePpm === null) return ''
  const absolute = Math.abs(ratePpm).toString().padStart(7, '0')
  const value = `${absolute.slice(0, -6)}.${absolute.slice(-6)}`.replace(/\.?0+$/, '')
  return `${ratePpm < 0 ? '−' : ''}${value || '0'}`
}

export function parseRateInput(value: string): number | null {
  const normalized = value.trim()
  if (!/^\d+(?:\.\d{1,6})?$/.test(normalized)) return null
  const [whole, fraction = ''] = normalized.split('.')
  const result = Number(`${whole}${fraction.padEnd(6, '0')}`)
  return Number.isSafeInteger(result) && result > 0 ? result : null
}
