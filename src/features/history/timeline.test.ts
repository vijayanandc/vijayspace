import { describe, expect, it } from 'vitest';

import type { StepRun } from '../../api/types';
import { describeRun, groupRuns } from './timeline';

function run(seq: number, key: string, status: StepRun['status'], extra: Partial<StepRun> = {}): StepRun {
  return { id: `stp_${seq}`, seq, step_key: key, kind: 'action', attempt: 1, status, input: null, output: null, error: null, warnings: [], started_at: '2026-01-01T00:00:00Z', finished_at: null, ...extra };
}

describe('timeline', () => {
  it('groups attempts and keeps the latest status', () => {
    const groups = groupRuns([run(2, 'a', 'retry_scheduled'), run(1, 'd', 'succeeded', { kind: 'decision', output: { branch: null } }), run(3, 'a', 'succeeded', { attempt: 2 })]);
    expect(groups.map((g) => `${g.stepKey}:${g.status}:${g.runs.length}`)).toEqual(['d:succeeded:1', 'a:succeeded:2']);
  });

  it('describes decisions and retries', () => {
    expect(describeRun(run(1, 'd', 'succeeded', { kind: 'decision', output: { branch: 'big' } }))).toBe('took branch “big”');
    expect(describeRun(run(1, 'd', 'succeeded', { kind: 'decision', output: { branch: null } }))).toBe('took “otherwise”');
    expect(describeRun(run(1, 'a', 'retry_scheduled', { attempt: 2 }))).toBe('attempt 2 failed; retry scheduled');
  });
});
