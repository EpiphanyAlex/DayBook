export function sliceByCodePoints(text: string, start: number, end: number): string {
  if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || start >= end) {
    throw new RangeError('字符区间必须是合法的左闭右开 code point 区间')
  }
  const points = Array.from(text)
  if (end > points.length) throw new RangeError('字符区间超出原文')
  return points.slice(start, end).join('')
}
