// The variable picker tree: what a step can reference (`trigger.*`, `steps.<key>.*`, `vars.*`).

import type { ConnectorManifest, FlowDefinition, JsonValue, Step } from '../../../api/types';

export interface VarNode {
  label: string;
  /** Template path, e.g. `steps.fetch.body.items[0].id`. Empty for group headers. */
  path: string;
  sample?: JsonValue;
  /** True when the step may not have run on every path (inside an earlier decision branch). */
  maybe?: boolean;
  children: VarNode[];
}

const IDENT = /^[A-Za-z_][A-Za-z0-9_]*$/;
const MAX_DEPTH = 5;
const MAX_KEYS = 60;

export function childPath(prefix: string, key: string | number): string {
  if (typeof key === 'number') return `${prefix}[${key}]`;
  if (IDENT.test(key)) return prefix ? `${prefix}.${key}` : key;
  return `${prefix}[${JSON.stringify(key)}]`;
}

/** Children of a JSON sample as variable nodes (arrays show their first element as `[0]`). */
export function jsonToVarNodes(prefix: string, value: JsonValue | undefined, depth = 0): VarNode[] {
  if (value === null || value === undefined || typeof value !== 'object' || depth >= MAX_DEPTH) return [];
  if (Array.isArray(value)) {
    if (value.length === 0) return [];
    const path = childPath(prefix, 0);
    return [{ label: '[0]', path, sample: value[0], children: jsonToVarNodes(path, value[0], depth + 1) }];
  }
  return Object.entries(value)
    .slice(0, MAX_KEYS)
    .map(([key, v]) => {
      const path = childPath(prefix, key);
      return { label: key, path, sample: v, children: jsonToVarNodes(path, v, depth + 1) };
    });
}

interface PriorStep {
  step: Step;
  maybe: boolean;
}

/** Steps that come before `key` on its path (earlier siblings of each enclosing list, and their nested steps). */
export function stepsBefore(def: FlowDefinition, key: string): PriorStep[] {
  const frames: { list: Step[]; index: number }[] = [];
  const find = (list: Step[]): boolean => {
    for (let i = 0; i < list.length; i += 1) {
      const s = list[i];
      if (!s) continue;
      frames.push({ list, index: i });
      if (s.key === key) return true;
      if (s.type === 'decision') {
        if (s.branches.some((b) => find(b.steps)) || find(s.otherwise)) return true;
      }
      frames.pop();
    }
    return false;
  };
  if (!find(def.steps)) return [];
  const out: PriorStep[] = [];
  const nested = (s: Step) => {
    if (s.type !== 'decision') return;
    const visit = (list: Step[]) =>
      list.forEach((n) => {
        out.push({ step: n, maybe: true });
        nested(n);
      });
    s.branches.forEach((b) => visit(b.steps));
    visit(s.otherwise);
  };
  for (const frame of frames) {
    for (const s of frame.list.slice(0, frame.index)) {
      out.push({ step: s, maybe: false });
      nested(s);
    }
  }
  return out;
}

function stepSample(step: Step, manifests: ConnectorManifest[]): JsonValue | undefined {
  if (step.sample_output !== undefined && step.sample_output !== null) return step.sample_output;
  if (step.type === 'action') {
    return manifests.find((m) => m.key === step.connector)?.actions.find((a) => a.key === step.action)?.output_sample;
  }
  if (step.type === 'decision') return { branch: 'branch_1' };
  return undefined;
}

function triggerSample(def: FlowDefinition, manifests: ConnectorManifest[], captured: JsonValue | undefined): JsonValue | undefined {
  if (captured !== undefined && captured !== null) return captured;
  const t = def.trigger;
  if (t.type === 'webhook') return { method: 'POST', headers: {}, query: {}, body: {} };
  if (t.type === 'schedule') return { scheduled_for: '2026-01-01T09:00:00Z', fired_at: '2026-01-01T09:00:01Z' };
  if (t.type === 'app') {
    return manifests.find((m) => m.key === t.connector)?.triggers.find((x) => x.key === t.trigger)?.output_sample;
  }
  return {};
}

function collectVars(def: FlowDefinition): string[] {
  const names = new Set<string>();
  const visit = (steps: Step[]) =>
    steps.forEach((s) => {
      if (s.type === 'set_variable') s.assignments.forEach((a) => a.name && names.add(a.name));
      if (s.type === 'decision') {
        s.branches.forEach((b) => visit(b.steps));
        visit(s.otherwise);
      }
    });
  visit(def.steps);
  return [...names].sort();
}

/** The full picker tree for the step `key` (or for the trigger / a new step when `key` is undefined). */
export function buildVariableTree(
  def: FlowDefinition,
  key: string | undefined,
  manifests: ConnectorManifest[],
  capturedTriggerSample?: JsonValue,
): VarNode[] {
  const tSample = triggerSample(def, manifests, capturedTriggerSample);
  const tree: VarNode[] = [{ label: 'Trigger', path: 'trigger', sample: tSample, children: jsonToVarNodes('trigger', tSample) }];
  const prior = key ? stepsBefore(def, key) : [];
  if (prior.length > 0) {
    tree.push({
      label: 'Steps',
      path: '',
      children: prior.map(({ step, maybe }) => {
        const sample = stepSample(step, manifests);
        const path = childPath('steps', step.key);
        return { label: step.name ? `${step.name} (${step.key})` : step.key, path, sample, maybe, children: jsonToVarNodes(path, sample) };
      }),
    });
  }
  const vars = collectVars(def);
  if (vars.length > 0) {
    tree.push({ label: 'Variables', path: '', children: vars.map((v) => ({ label: v, path: childPath('vars', v), children: [] })) });
  }
  tree.push({
    label: 'Execution',
    path: '',
    children: [
      { label: 'id', path: 'execution.id', children: [] },
      { label: 'started_at', path: 'execution.started_at', children: [] },
      { label: 'flow name', path: 'flow.name', children: [] },
    ],
  });
  return tree;
}

/** The `context` for `POST /steps/{key}/test`: trigger sample and samples of earlier steps. */
export function testContext(
  def: FlowDefinition,
  key: string,
  manifests: ConnectorManifest[],
  capturedTriggerSample?: JsonValue,
): { [k: string]: JsonValue } {
  const steps: { [k: string]: JsonValue } = {};
  for (const { step } of stepsBefore(def, key)) {
    const sample = stepSample(step, manifests);
    if (sample !== undefined) steps[step.key] = sample;
  }
  const vars: { [k: string]: JsonValue } = {};
  return { trigger: triggerSample(def, manifests, capturedTriggerSample) ?? {}, steps, vars };
}
