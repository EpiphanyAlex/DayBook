import { describe, expect, it } from 'vitest'
import { canBatchConfirm, utteranceBatchGateReady } from './policy'

describe('review/utterance-batch-gate', () => {
  const ready = {
    fullSourceVisible: true,
    resultsAdjacent: true,
    itemCountVisible: true,
  }

  it('requires all three visible review conditions', () => {
    expect(utteranceBatchGateReady(ready)).toBe(true)
    for (const key of Object.keys(ready) as (keyof typeof ready)[]) {
      expect(utteranceBatchGateReady({ ...ready, [key]: false })).toBe(false)
    }
  })

  it('does not turn a single-only policy into a batch action', () => {
    expect(canBatchConfirm({
      reconciliationStatus: 'passed',
      confirmationPolicy: 'single_only',
    }, ready)).toBe(false)
  })
})
