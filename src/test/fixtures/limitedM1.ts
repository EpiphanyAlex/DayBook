import { parseIpcIntegers, serializeIpcIntegers, type MinorUnits, type RatePpm } from '../../lib/bridge'
import type { ReviewSource, ReviewDraft, TotalCheck, EvidenceContent } from '../../review/types'

export const source = (id = 'A', attempt = `${id}-1`): ReviewSource => ({
  id, kind: 'utterance', originalFilename: null, ext: 'txt', state: 'parsed',
  parseErrorCode: null, latestAttemptId: attempt, importedAt: '2026-09-06T00:00:00Z', activeDraftCount: 2,
})
export const original = '周六☕咖啡18元，午饭24元。'
export const evidence = (): EvidenceContent => ({ kind: 'utterance', mimeType: 'text/plain', text: original, dataBase64: null })
export const draft = (id = 'A-d1', sourceId = 'A', attemptId = `${sourceId}-1`): ReviewDraft => ({
  id, sourceId, attemptId, evidenceText: '☕咖啡18元', evidenceSpanStart: 2, evidenceSpanEnd: 8,
  sourceOrdinal: 1, occurredOn: '2026-09-05', amountMinor: 1800 as MinorUnits, currency: 'CNY',
  baseAmountMinor: 1800 as MinorUnits, baseCurrency: 'CNY', ratePpm: 1000000 as RatePpm,
  direction: 'expense', merchant: id, category: '餐饮', channel: null, confidence: null,
})
export const total = (sourceId = 'A', attemptId = `${sourceId}-1`): TotalCheck => ({
  sourceId, attemptId, sourceKind: 'utterance', reconciliationStatus: 'not_applicable',
  confirmationPolicy: 'user_attested_batch', reportedTotalMinor: null, calculatedTotalMinor: null,
  reportedTotalCurrency: null, reportedTotalKind: null, reportedTotalEvidenceText: null,
  unavailableDraftIds: [], outcome: 'completed', unparsedNote: null,
})
export function fixtureHandlers() {
  const state = {
    sources: [source(), source('B')],
    drafts: [draft(), draft('A-d2'), draft('B-d1', 'B')],
    totals: [total(), total('B')],
  }
  const handlers = {
    foundation_status: () => ({ schemaVersion: 6, dataDirectory: '/synthetic/daybook', baseCurrency: 'CNY', debugLogging: false }),
    agent_status: () => ({ available: true, availabilityReason: null, authenticated: true, ready: true, errorCode: null, version: 'mock' }),
    probe_agent: () => handlers.agent_status(),
    recent_agent_logs: () => [],
    list_review_sources: () => structuredClone(state.sources),
    list_active_drafts: ({ sourceId }: Record<string, unknown>) => serializeIpcIntegers(state.drafts.filter(d => d.sourceId === sourceId)),
    read_evidence: () => evidence(),
    check_source_total: ({ attemptId }: Record<string, unknown>) => serializeIpcIntegers(state.totals.find(t => t.attemptId === attemptId)),
    update_draft: ({ draftId, patch }: Record<string, unknown>) => {
      const row = state.drafts.find(d => d.id === draftId)
      if (row) Object.assign(row, parseIpcIntegers(patch))
      return null
    },
    discard_draft: () => null,
    confirm_draft: () => null,
    confirm_drafts: () => ({ confirmed: [], rejected: [] }),
  }
  return { state, handlers }
}
