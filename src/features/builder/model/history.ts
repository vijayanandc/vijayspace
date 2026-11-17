// Undo/redo history for the builder's definition.

import type { FlowDefinition } from '../../../api/types';

const LIMIT = 100;

export interface History {
  past: FlowDefinition[];
  present: FlowDefinition;
  future: FlowDefinition[];
  /** Edits with the same tag in a row are coalesced into one undo step (e.g. typing in one field). */
  lastTag?: string;
}

export type HistoryAction =
  | { type: 'apply'; def: FlowDefinition; tag?: string }
  | { type: 'undo' }
  | { type: 'redo' }
  | { type: 'reset'; def: FlowDefinition };

export function initHistory(def: FlowDefinition): History {
  return { past: [], present: def, future: [] };
}

export function historyReducer(state: History, action: HistoryAction): History {
  switch (action.type) {
    case 'reset':
      return initHistory(action.def);
    case 'apply': {
      if (action.def === state.present) return state;
      if (action.tag !== undefined && action.tag === state.lastTag) {
        return { ...state, present: action.def, future: [] };
      }
      const past = [...state.past, state.present].slice(-LIMIT);
      return { past, present: action.def, future: [], lastTag: action.tag };
    }
    case 'undo': {
      const previous = state.past[state.past.length - 1];
      if (!previous) return state;
      return { past: state.past.slice(0, -1), present: previous, future: [state.present, ...state.future] };
    }
    case 'redo': {
      const next = state.future[0];
      if (!next) return state;
      return { past: [...state.past, state.present], present: next, future: state.future.slice(1) };
    }
  }
}
