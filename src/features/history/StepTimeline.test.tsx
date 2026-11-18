import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { StepRun } from '../../api/types';
import { renderWithProviders } from '../../test/render';
import { StepTimeline } from './StepTimeline';

const run = (key: string, input: StepRun['input'], output: StepRun['output']): StepRun => ({
  id: `sr_${key}`, seq: 1, step_key: key, kind: 'action', attempt: 1, status: 'succeeded', input, output, error: null, warnings: [],
  started_at: '2026-11-18T10:00:00Z', finished_at: '2026-11-18T10:00:01Z',
});

describe('step timeline', () => {
  it('shows a custom function’s print lines as a log', () => {
    renderWithProviders(<StepTimeline steps={[run('fn', { code: 'print("hi"); #{}', input: {} }, { total: 1, _logs: ['hi'] })]} />);
    expect(screen.getByRole('region', { name: 'Log' })).toHaveTextContent('hi');
  });

  it('keeps an `_logs` field of another app in its output', () => {
    renderWithProviders(<StepTimeline steps={[run('http', { method: 'GET', url: 'https://x' }, { _logs: ['from the API'] })]} />);
    expect(screen.queryByRole('region', { name: 'Log' })).not.toBeInTheDocument();
    expect(screen.getByText(/"_logs"/)).toBeInTheDocument();
  });
});
