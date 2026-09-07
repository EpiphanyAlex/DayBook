import { sliceByCodePoints } from '../lib/text'
import type { ReviewDraft } from './types'

export function verifiedSpan(text: string, draft: ReviewDraft): [string, string, string] | null {
  const { evidenceSpanStart: start, evidenceSpanEnd: end } = draft
  if (start === null || end === null || !draft.evidenceText) return null
  try {
    const selected = sliceByCodePoints(text, start, end)
    if (selected !== draft.evidenceText) return null
    const points = Array.from(text)
    return [points.slice(0, start).join(''), selected, points.slice(end).join('')]
  } catch { return null }
}
