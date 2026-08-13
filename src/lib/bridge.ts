import { invoke } from '@tauri-apps/api/core'

export type MinorUnits = number & { readonly __minorUnits: unique symbol }
export type RatePpm = number & { readonly __ratePpm: unique symbol }

export interface AppErrorShape {
  code: string
  message: string
  detail: unknown | null
}

export class AppError extends Error implements AppErrorShape {
  readonly code: string
  readonly detail: unknown | null

  constructor(shape: AppErrorShape) {
    super(shape.message)
    this.name = 'AppError'
    this.code = shape.code
    this.detail = shape.detail
  }
}

const INTEGER_FIELDS = new Set([
  'amountMinor',
  'baseAmountMinor',
  'ratePpm',
  'reportedTotalMinor',
  'calculatedTotalMinor',
])

export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    const value: unknown = await invoke(command, serializeIpcIntegers(args) as Record<string, unknown>)
    return parseIpcIntegers(value) as T
  } catch (error: unknown) {
    throw normalizeError(error)
  }
}

export function serializeIpcIntegers(value: unknown, fieldName?: string): unknown {
  if (fieldName && INTEGER_FIELDS.has(fieldName)) {
    if (value === null) return null
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || Math.abs(value) > 1e15) {
      throw new AppError({
        code: 'data.amount_out_of_range',
        message: '金额或汇率超出支持范围',
        detail: null,
      })
    }
    return value.toString()
  }
  if (Array.isArray(value)) return value.map((item) => serializeIpcIntegers(item))
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, serializeIpcIntegers(item, key)]),
    )
  }
  return value
}

export function parseIpcIntegers(value: unknown, fieldName?: string): unknown {
  if (fieldName && INTEGER_FIELDS.has(fieldName)) return parseIntegerString(value)
  if (Array.isArray(value)) return value.map((item) => parseIpcIntegers(item))
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, parseIpcIntegers(item, key)]),
    )
  }
  return value
}

function parseIntegerString(value: unknown): number | null {
  if (value === null) return null
  if (typeof value !== 'string' || !/^-?(0|[1-9]\d*)$/.test(value)) {
    throw new AppError({
      code: 'data.invalid_argument',
      message: 'IPC 金额不是规范十进制整数字符串',
      detail: null,
    })
  }
  const parsed = Number(value)
  if (!Number.isSafeInteger(parsed) || Math.abs(parsed) > 1e15) {
    throw new AppError({
      code: 'data.amount_out_of_range',
      message: '金额或汇率超出支持范围',
      detail: null,
    })
  }
  return parsed
}

function normalizeError(error: unknown): AppError {
  if (error instanceof AppError) return error
  if (isAppErrorShape(error)) return new AppError(error)
  return new AppError({
    code: 'data.storage_failure',
    message: error instanceof Error ? error.message : '发生未知错误',
    detail: null,
  })
}

function isAppErrorShape(value: unknown): value is AppErrorShape {
  if (value === null || typeof value !== 'object') return false
  const candidate = value as Record<string, unknown>
  return typeof candidate.code === 'string' && typeof candidate.message === 'string'
}
