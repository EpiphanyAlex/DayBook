export interface EditBuffer { values: Record<string, string>; revision: number; error: string | null }
export interface ScreenState {
  selectedId: string | null
  selectionRevision: number
  focusedByAttempt: Record<string, string>
  excludedByAttempt: Record<string, string[]>
  edits: Record<string, EditBuffer>
}
export const initialScreen: ScreenState = { selectedId: null, selectionRevision: 0, focusedByAttempt: {}, excludedByAttempt: {}, edits: {} }
export const attemptKey = (sourceId: string, attemptId: string | null) => JSON.stringify([sourceId, attemptId])
export type ScreenAction =
  | { type: 'select'; id: string | null }
  | { type: 'focus'; scope: string; id: string }
  | { type: 'exclude'; scope: string; ids: string[] }
  | { type: 'edit'; key: string; field: string; value: string }
  | { type: 'saved'; key: string; revision: number; fields: string[] }
  | { type: 'edit-error'; key: string; revision: number; error: string }
  | { type: 'restore'; key: string }
export function screenReducer(state: ScreenState, action: ScreenAction): ScreenState {
  switch (action.type) {
    case 'select': return { ...state, selectedId: action.id, selectionRevision: state.selectionRevision + 1 }
    case 'focus': return { ...state, focusedByAttempt: { ...state.focusedByAttempt, [action.scope]: action.id } }
    case 'exclude': return { ...state, excludedByAttempt: { ...state.excludedByAttempt, [action.scope]: action.ids } }
    case 'edit': {
      const old = state.edits[action.key]
      return { ...state, edits: { ...state.edits, [action.key]: { values: { ...old?.values, [action.field]: action.value }, revision: (old?.revision ?? 0) + 1, error: null } } }
    }
    case 'restore': return { ...state, edits: { ...state.edits, [action.key]: { values: {}, revision: (state.edits[action.key]?.revision ?? 0) + 1, error: null } } }
    case 'edit-error': {
      const old = state.edits[action.key]
      if (!old || old.revision !== action.revision) return state
      return { ...state, edits: { ...state.edits, [action.key]: { ...old, error: action.error } } }
    }
    case 'saved': {
      const old = state.edits[action.key]
      if (!old || old.revision !== action.revision) return state
      const values = { ...old.values }
      for (const field of action.fields) delete values[field]
      return { ...state, edits: { ...state.edits, [action.key]: { ...old, values, error: null } } }
    }
  }
}
