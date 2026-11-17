// Groups step runs by step for the execution timeline (attempts stay together, in first-seen order).

import type { StepRun } from '../../api/types';

export interface StepGroup {
  stepKey: string;
  kind: string;
  runs: StepRun[];
  /** The latest attempt decides the shown status. */
  status: StepRun['status'];
}

export function groupRuns(runs: StepRun[]): StepGroup[] {
  const sorted = [...runs].sort((a, b) => a.seq - b.seq);
  const groups: StepGroup[] = [];
  const byKey = new Map<string, StepGroup>();
  for (const run of sorted) {
    let group = byKey.get(run.step_key);
    if (!group) {
      group = { stepKey: run.step_key, kind: run.kind, runs: [], status: run.status };
      byKey.set(run.step_key, group);
      groups.push(group);
    }
    group.runs.push(run);
    group.status = run.status;
  }
  return groups;
}

/** Short description of a step run's result (branch taken, delay until…). */
export function describeRun(run: StepRun): string {
  const out = run.output;
  if (run.kind === 'decision' && out && typeof out === 'object' && !Array.isArray(out)) {
    const branch = out.branch;
    return typeof branch === 'string' ? `took branch “${branch}”` : 'took “otherwise”';
  }
  if (run.kind === 'delay' && out && typeof out === 'object' && !Array.isArray(out) && typeof out.until === 'string') {
    return `until ${new Date(out.until).toLocaleString()}`;
  }
  if (run.status === 'retry_scheduled') return `attempt ${run.attempt} failed; retry scheduled`;
  return '';
}
