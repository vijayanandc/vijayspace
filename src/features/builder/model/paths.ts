// JSON Pointer paths from validation issues → the node they belong to.

import type { Branch, FlowDefinition, Issue, Step } from '../../../api/types';

export const TRIGGER_ID = 'trigger';

/** Maps an issue path like `/steps/0/branches/1/steps/0/input/url` to the deepest step key. */
export function nodeForPointer(def: FlowDefinition, pointer: string): string | undefined {
  const parts = pointer.split('/').slice(1);
  if (parts[0] === 'trigger') return TRIGGER_ID;
  let list: Step[] | undefined = def.steps;
  let found: string | undefined;
  let i = 0;
  while (i < parts.length && list) {
    const token = parts[i];
    if (token === 'steps' || token === 'otherwise') {
      const step: Step | undefined = list[Number(parts[i + 1])];
      if (!step) break;
      found = step.key;
      i += 2;
      if (step.type !== 'decision') break;
      const next = parts[i];
      if (next === 'branches') {
        const branch: Branch | undefined = step.branches[Number(parts[i + 1])];
        list = branch?.steps;
        i += 2;
      } else if (next === 'otherwise') {
        list = step.otherwise;
      } else {
        break;
      }
    } else {
      break;
    }
  }
  return found;
}

/** Issues grouped by node id (step key or `trigger`). */
export function issuesByNode(def: FlowDefinition, issues: Issue[]): Map<string, Issue[]> {
  const map = new Map<string, Issue[]>();
  for (const issue of issues) {
    const id = nodeForPointer(def, issue.path);
    if (!id) continue;
    const list = map.get(id) ?? [];
    list.push(issue);
    map.set(id, list);
  }
  return map;
}
