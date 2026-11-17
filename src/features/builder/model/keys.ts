// Step and branch keys: `[a-z][a-z0-9_]{0,63}`, unique in the flow.

import type { DecisionStep, FlowDefinition, Step } from '../../../api/types';

const KEY_RE = /^[a-z][a-z0-9_]{0,63}$/;

export function isValidKey(key: string): boolean {
  return KEY_RE.test(key);
}

/** Turns any label into a key-shaped string (`Send Email!` → `send_email`). */
export function slugKey(base: string): string {
  let k = base
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '');
  if (!k) k = 'step';
  if (!/^[a-z]/.test(k)) k = `s_${k}`;
  return k.slice(0, 56);
}

export function allKeys(def: FlowDefinition): Set<string> {
  const keys = new Set<string>();
  const visit = (steps: Step[]) => {
    for (const s of steps) {
      keys.add(s.key);
      if (s.type === 'decision') {
        s.branches.forEach((b) => visit(b.steps));
        visit(s.otherwise);
      }
    }
  };
  visit(def.steps);
  return keys;
}

/** A key not used anywhere in the flow: `send_email`, `send_email_2`, … */
export function uniqueKey(def: FlowDefinition, base: string): string {
  const taken = allKeys(def);
  const k = slugKey(base);
  if (!taken.has(k)) return k;
  for (let n = 2; ; n += 1) {
    const candidate = `${k}_${n}`;
    if (!taken.has(candidate)) return candidate;
  }
}

export function uniqueBranchKey(decision: DecisionStep): string {
  const taken = new Set(decision.branches.map((b) => b.key));
  for (let n = decision.branches.length + 1; ; n += 1) {
    const candidate = `branch_${n}`;
    if (!taken.has(candidate)) return candidate;
  }
}
