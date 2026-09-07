import type { MinorUnits, RatePpm } from '../lib/bridge'
import type { ReviewPolicy, SourceStateValue } from './policy'

export interface FoundationStatus {
  schemaVersion: number
  dataDirectory: string
  baseCurrency: string | null
  debugLogging: boolean
}

export interface BackendStatus {
  available: boolean
  availabilityReason: string | null
  authenticated: boolean | null
  ready: boolean
  errorCode: string | null
  version: string | null
}

export interface ReviewSource {
  id: string
  kind: 'file' | 'utterance'
  originalFilename: string | null
  ext: string
  state: SourceStateValue
  parseErrorCode: string | null
  latestAttemptId: string | null
  importedAt: string
  activeDraftCount: number
}

export interface ReviewDraft {
  id: string
  sourceId: string
  attemptId: string
  evidenceText: string
  sourceOrdinal: number
  evidenceSpanStart: number | null
  evidenceSpanEnd: number | null
  occurredOn: string
  amountMinor: MinorUnits
  currency: string
  baseAmountMinor: MinorUnits | null
  baseCurrency: string | null
  ratePpm: RatePpm | null
  direction: 'expense' | 'income' | 'transfer'
  merchant: string
  category: string | null
  channel: string | null
  confidence: number | null
}

export interface TotalCheck extends ReviewPolicy {
  attemptId: string
  sourceId: string
  sourceKind: string
  reportedTotalMinor: MinorUnits | null
  calculatedTotalMinor: MinorUnits | null
  reportedTotalCurrency: string | null
  reportedTotalKind: string | null
  reportedTotalEvidenceText: string | null
  unavailableDraftIds: string[]
  outcome: string | null
  unparsedNote: string | null
}

export interface EvidenceContent {
  kind: 'file' | 'utterance'
  mimeType: string
  dataBase64: string | null
  text: string | null
}

export interface ImportResult {
  sourceId: string
  deduplicated: boolean
  matchingUtteranceSourceIds: string[]
}
