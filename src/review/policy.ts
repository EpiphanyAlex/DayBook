export interface ReviewPolicy {
  reconciliationStatus: 'passed' | 'failed' | 'unavailable' | 'not_applicable'
  confirmationPolicy: 'reconciled_batch' | 'user_attested_batch' | 'single_only'
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
  if (policy.confirmationPolicy === 'single_only') return false
  if (policy.confirmationPolicy === 'user_attested_batch') {
    return utteranceBatchGateReady(gate)
  }
  return true
}
