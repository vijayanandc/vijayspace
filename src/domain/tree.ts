// Pure, immutable operations on the flow definition tree.

import type { Branch, DecisionStep, FlowDefinition, Step } from '../api/types';
import { defaultCondition } from './factories';
import { uniqueBranchKey } from './keys';

/** Identifies one list of steps in the tree. */
export type ListRef =
  | { kind: 'root' }
  | { kind: 'branch'; decision: string; branch: string }
  | { kind: 'otherwise'; decision: string };

/** A position between steps where a new step can be inserted. */
export interface InsertionPoint {
  list: ListRef;
  index: number;
}

export const ROOT: ListRef = { kind: 'root' };

export function listRefId(ref: ListRef): string {
  switch (ref.kind) {
    case 'root':
      return 'root';
    case 'branch':
      return `b:${ref.decision}:${ref.branch}`;
    case 'otherwise':
      return `o:${ref.decision}`;
  }
}

export function sameList(a: ListRef, b: ListRef): boolean {
  return listRefId(a) === listRefId(b);
}

/** Applies `f` to every decision with key `key`, anywhere in the tree. */
function mapDecision(steps: Step[], key: string, f: (d: DecisionStep) => DecisionStep): Step[] {
  return steps.map((s) => {
    if (s.type !== 'decision') return s;
    if (s.key === key) return f(s);
    return {
      ...s,
      branches: s.branches.map((b) => ({ ...b, steps: mapDecision(b.steps, key, f) })),
      otherwise: mapDecision(s.otherwise, key, f),
    };
  });
}

/** The same array when every element is the same object (so unchanged parts of the tree keep their identity). */
function sameItems<T>(before: T[], after: T[]): T[] {
  return after.every((x, i) => x === before[i]) ? before : after;
}

/**
 * Applies `f` to every step (pre-order); decisions are recursed after mapping. Steps, branches and lists that did
 * not change keep their objects: the canvas and memoised views compare by identity.
 */
export function mapDeep(steps: Step[], f: (s: Step) => Step): Step[] {
  return sameItems(
    steps,
    steps.map((s) => {
      const mapped = f(s);
      if (mapped.type !== 'decision') return mapped;
      const branches = sameItems(
        mapped.branches,
        mapped.branches.map((b) => {
          const inner = mapDeep(b.steps, f);
          return inner === b.steps ? b : { ...b, steps: inner };
        }),
      );
      const otherwise = mapDeep(mapped.otherwise, f);
      if (branches === mapped.branches && otherwise === mapped.otherwise) return mapped;
      return { ...mapped, branches, otherwise };
    }),
  );
}

export function updateList(def: FlowDefinition, ref: ListRef, f: (list: Step[]) => Step[]): FlowDefinition {
  switch (ref.kind) {
    case 'root':
      return { ...def, steps: f(def.steps) };
    case 'branch':
      return {
        ...def,
        steps: mapDecision(def.steps, ref.decision, (d) => ({
          ...d,
          branches: d.branches.map((b) => (b.key === ref.branch ? { ...b, steps: f(b.steps) } : b)),
        })),
      };
    case 'otherwise':
      return { ...def, steps: mapDecision(def.steps, ref.decision, (d) => ({ ...d, otherwise: f(d.otherwise) })) };
  }
}

export interface Location {
  ref: ListRef;
  index: number;
  step: Step;
}

function locateIn(steps: Step[], ref: ListRef, key: string): Location | undefined {
  for (let i = 0; i < steps.length; i += 1) {
    const s = steps[i];
    if (!s) continue;
    if (s.key === key) return { ref, index: i, step: s };
    if (s.type === 'decision') {
      for (const b of s.branches) {
        const found = locateIn(b.steps, { kind: 'branch', decision: s.key, branch: b.key }, key);
        if (found) return found;
      }
      const found = locateIn(s.otherwise, { kind: 'otherwise', decision: s.key }, key);
      if (found) return found;
    }
  }
  return undefined;
}

/** Finds a step anywhere in the tree. */
export function locate(def: FlowDefinition, key: string): Location | undefined {
  return locateIn(def.steps, ROOT, key);
}

export function findStep(def: FlowDefinition, key: string): Step | undefined {
  return locate(def, key)?.step;
}

/** The steps of a list, or undefined if the list does not exist. */
export function getList(def: FlowDefinition, ref: ListRef): Step[] | undefined {
  if (ref.kind === 'root') return def.steps;
  const d = findStep(def, ref.decision);
  if (!d || d.type !== 'decision') return undefined;
  if (ref.kind === 'otherwise') return d.otherwise;
  return d.branches.find((b) => b.key === ref.branch)?.steps;
}

export function insertStep(def: FlowDefinition, at: InsertionPoint, step: Step): FlowDefinition {
  if (!getList(def, at.list)) return def;
  return updateList(def, at.list, (list) => {
    const idx = Math.max(0, Math.min(at.index, list.length));
    return [...list.slice(0, idx), step, ...list.slice(idx)];
  });
}

export function deleteStep(def: FlowDefinition, key: string): FlowDefinition {
  const loc = locate(def, key);
  if (!loc) return def;
  return updateList(def, loc.ref, (list) => list.filter((s) => s.key !== key));
}

/** Keys of all decisions inside `step` (including itself). */
function decisionKeysWithin(step: Step): Set<string> {
  const keys = new Set<string>();
  const visit = (s: Step) => {
    if (s.type !== 'decision') return;
    keys.add(s.key);
    s.branches.forEach((b) => b.steps.forEach(visit));
    s.otherwise.forEach(visit);
  };
  visit(step);
  return keys;
}

/** Moves a step to an insertion point. Moving a decision into itself is refused (returns `def`). */
export function moveStep(def: FlowDefinition, key: string, to: InsertionPoint): FlowDefinition {
  const loc = locate(def, key);
  if (!loc || !getList(def, to.list)) return def;
  if (to.list.kind !== 'root' && decisionKeysWithin(loc.step).has(to.list.decision)) return def;
  let index = to.index;
  if (sameList(loc.ref, to.list)) {
    if (index === loc.index || index === loc.index + 1) return def;
    if (index > loc.index) index -= 1;
  }
  return insertStep(deleteStep(def, key), { list: to.list, index }, loc.step);
}

export function updateStep(def: FlowDefinition, key: string, f: (s: Step) => Step): FlowDefinition {
  return { ...def, steps: mapDeep(def.steps, (s) => (s.key === key ? f(s) : s)) };
}

// ---------- Branches ----------

export function addBranch(def: FlowDefinition, decision: string): FlowDefinition {
  return {
    ...def,
    steps: mapDecision(def.steps, decision, (d) => {
      const key = uniqueBranchKey(d);
      const branch: Branch = { key, name: `Branch ${d.branches.length + 1}`, condition: defaultCondition(), steps: [] };
      return { ...d, branches: [...d.branches, branch] };
    }),
  };
}

export function removeBranch(def: FlowDefinition, decision: string, branch: string): FlowDefinition {
  return {
    ...def,
    steps: mapDecision(def.steps, decision, (d) => ({ ...d, branches: d.branches.filter((b) => b.key !== branch) })),
  };
}

export function updateBranch(def: FlowDefinition, decision: string, branch: string, f: (b: Branch) => Branch): FlowDefinition {
  return {
    ...def,
    steps: mapDecision(def.steps, decision, (d) => ({ ...d, branches: d.branches.map((b) => (b.key === branch ? f(b) : b)) })),
  };
}

export function moveBranch(def: FlowDefinition, decision: string, branch: string, delta: -1 | 1): FlowDefinition {
  return {
    ...def,
    steps: mapDecision(def.steps, decision, (d) => {
      const i = d.branches.findIndex((b) => b.key === branch);
      const j = i + delta;
      if (i < 0 || j < 0 || j >= d.branches.length) return d;
      const branches = [...d.branches];
      const a = branches[i];
      const b = branches[j];
      if (!a || !b) return d;
      branches[i] = b;
      branches[j] = a;
      return { ...d, branches };
    }),
  };
}

/** Every step, depth-first. */
export function allSteps(def: FlowDefinition): Step[] {
  const out: Step[] = [];
  const visit = (steps: Step[]) => {
    for (const s of steps) {
      out.push(s);
      if (s.type === 'decision') {
        s.branches.forEach((b) => visit(b.steps));
        visit(s.otherwise);
      }
    }
  };
  visit(def.steps);
  return out;
}
