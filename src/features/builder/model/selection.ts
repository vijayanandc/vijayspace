// Keeping the builder's selection valid when the definition changes underneath it (undo, redo, deletes).

import type { FlowDefinition } from '../../../api/types';
import { TRIGGER_ID } from './paths';
import { allSteps } from '../../../domain/tree';

/**
 * The node to keep selected after `prev` became `next`: the same key when it still exists, the step's new
 * key when exactly that step was renamed (e.g. undoing a key rename), and nothing otherwise.
 */
export function followSelection(prev: FlowDefinition, next: FlowDefinition, selected: string | null): string | null {
  if (selected === null || selected === TRIGGER_ID) return selected;
  const nextKeys = new Set(allSteps(next).map((s) => s.key));
  if (nextKeys.has(selected)) return selected;
  const prevKeys = new Set(allSteps(prev).map((s) => s.key));
  const added = [...nextKeys].filter((k) => !prevKeys.has(k));
  const removed = [...prevKeys].filter((k) => !nextKeys.has(k));
  return added.length === 1 && removed.length === 1 && removed[0] === selected ? (added[0] ?? null) : null;
}
