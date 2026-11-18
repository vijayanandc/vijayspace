import { describe, expect, it } from 'vitest';

import type { Step } from '../../../api/types';
import { stepLabel } from './labels';

describe('step labels', () => {
  it('labels a custom function whose input is missing (older or hand-edited definitions)', () => {
    const step = { key: 'fn', type: 'action', connector: 'core.script', action: 'run', input: null } as unknown as Step;
    expect(stepLabel(step, [])).toMatchObject({ title: 'Custom function', subtitle: 'Rhai · No code yet' });
  });

  it('counts the lines of a custom function', () => {
    const step: Step = { key: 'fn', name: 'Total', type: 'action', connector: 'core.script', action: 'run', input: { code: 'let a = 1;\na\n' } };
    expect(stepLabel(step, []).subtitle).toBe('Rhai · 2 lines');
  });
});
