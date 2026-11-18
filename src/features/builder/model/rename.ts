// Renaming a step key, updating `steps.<key>` references everywhere.

import type { FlowDefinition, JsonValue, Step } from '../../../api/types';
import { isValidKey } from '../../../domain/keys';
import { allSteps, mapDeep } from '../../../domain/tree';

function replaceRefs(value: JsonValue, from: RegExp, to: string): JsonValue {
  if (typeof value === 'string') return value.replace(from, to);
  if (Array.isArray(value)) return value.map((v) => replaceRefs(v, from, to));
  if (value && typeof value === 'object') {
    const out: { [k: string]: JsonValue } = {};
    for (const [k, v] of Object.entries(value)) out[k] = replaceRefs(v, from, to);
    return out;
  }
  return value;
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Renames `oldKey` to `newKey` (no-op when invalid or taken) and rewrites references. */
export function renameStep(def: FlowDefinition, oldKey: string, newKey: string): FlowDefinition {
  if (oldKey === newKey || !isValidKey(newKey)) return def;
  if (allSteps(def).some((s) => s.key === newKey)) return def;
  const re = new RegExp(`\\bsteps\\.${escapeRe(oldKey)}\\b`, 'g');
  const to = `steps.${newKey}`;
  const rewrite = (s: Step): Step => {
    const renamed: Step = s.key === oldKey ? { ...s, key: newKey } : s;
    switch (renamed.type) {
      case 'action':
        return { ...renamed, input: replaceRefs(renamed.input, re, to) as typeof renamed.input };
      case 'set_variable':
        return { ...renamed, assignments: renamed.assignments.map((a) => ({ ...a, value: replaceRefs(a.value, re, to) })) };
      case 'delay':
        return renamed.until ? { ...renamed, until: renamed.until.replace(re, to) } : renamed;
      case 'decision':
        return {
          ...renamed,
          branches: renamed.branches.map((b) => ({
            ...b,
            condition: replaceRefs(b.condition as unknown as JsonValue, re, to) as unknown as typeof b.condition,
          })),
        };
      case 'stop':
        return renamed;
    }
  };
  return { ...def, steps: mapDeep(def.steps, rewrite) };
}
