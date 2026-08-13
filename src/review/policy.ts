/** 能不能对账（四态）。状态字符串集中定义，不在组件里散写字面量。 */
export const RECONCILIATION = {
  PASSED: 'passed',
  FAILED: 'failed',
  UNAVAILABLE: 'unavailable',
  NOT_APPLICABLE: 'not_applicable',
} as const

/** 能不能批量确认（三态）。**放行批量的是这个，不是上面那个。** */
export const POLICY = {
  RECONCILED_BATCH: 'reconciled_batch',
  USER_ATTESTED_BATCH: 'user_attested_batch',
  SINGLE_ONLY: 'single_only',
} as const

export const SOURCE_STATE = {
  IMPORTED: 'imported',
  PARSING: 'parsing',
  PARSED: 'parsed',
  FAILED: 'failed',
  REVIEWED: 'reviewed',
} as const

export type ReconciliationStatus = (typeof RECONCILIATION)[keyof typeof RECONCILIATION]
export type ConfirmationPolicy = (typeof POLICY)[keyof typeof POLICY]
export type SourceStateValue = (typeof SOURCE_STATE)[keyof typeof SOURCE_STATE]

export interface ReviewPolicy {
  reconciliationStatus: ReconciliationStatus
  confirmationPolicy: ConfirmationPolicy
}

export interface UtteranceGateState {
  fullSourceVisible: boolean
  resultsAdjacent: boolean
  itemCountVisible: boolean
}

export function utteranceBatchGateReady(state: UtteranceGateState): boolean {
  return state.fullSourceVisible && state.resultsAdjacent && state.itemCountVisible
}

export function canBatchConfirm(policy: ReviewPolicy, gate: UtteranceGateState): boolean {
  if (policy.confirmationPolicy === POLICY.SINGLE_ONLY) return false
  if (policy.confirmationPolicy === POLICY.USER_ATTESTED_BATCH) {
    return utteranceBatchGateReady(gate)
  }
  return true
}
