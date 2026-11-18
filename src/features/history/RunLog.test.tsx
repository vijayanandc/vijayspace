// The run log view: lines from the server in order, filters by problems and step, details with the app's answer.

import { fireEvent, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ExecutionLog } from '../../api/types';
import { renderWithProviders } from '../../test/render';
import { RunLog } from './RunLog';

const log: ExecutionLog = {
  truncated: false,
  entries: [
    { at: '2026-11-18T10:00:00.000Z', level: 'info', kind: 'run', step_key: null, attempt: null, message: 'Run queued (live)', details: null },
    { at: '2026-11-18T10:00:00.020Z', level: 'info', kind: 'step', step_key: 'crm', attempt: 1, message: 'Step `crm` started (attempt 1)', details: null },
    {
      at: '2026-11-18T10:00:00.182Z',
      level: 'warn',
      kind: 'http',
      step_key: 'crm',
      attempt: 1,
      message: 'POST https://crm.example.com/v2/Leads → 503 in 162 ms',
      details: { method: 'POST', url: 'https://crm.example.com/v2/Leads', status: 503, error: null, duration_ms: 162, request_bytes: 120, response_bytes: 15, response_excerpt: 'busy, try later' },
    },
    { at: '2026-11-18T10:00:02.400Z', level: 'info', kind: 'step', step_key: 'mail', attempt: 1, message: 'Step `mail` succeeded in 12 ms', details: null },
  ],
};

function open() {
  const fetchMock = vi.fn(async (_url: RequestInfo | URL) => new Response(JSON.stringify(log)));
  vi.stubGlobal('fetch', fetchMock);
  renderWithProviders(<RunLog executionId="exe_1" status="succeeded" waitingUntil={null} />);
  return fetchMock;
}

describe('run log', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('lists every line in order with its offset from the start', async () => {
    const fetchMock = open();
    const lines = await screen.findByRole('list', { name: 'Log lines' });
    const items = within(lines).getAllByRole('listitem');
    expect(items.map((li) => li.textContent)).toEqual([
      expect.stringContaining('Run queued (live)'),
      expect.stringContaining('Step `crm` started (attempt 1)'),
      expect.stringContaining('POST https://crm.example.com/v2/Leads → 503 in 162 ms'),
      expect.stringContaining('Step `mail` succeeded'),
    ]);
    expect(items[2]).toHaveTextContent('+182 ms');
    expect(items[2]).toHaveTextContent('Warning');
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/api/v1/executions/exe_1/log');
  });

  it('filters to problems or one step and shows a call’s details and the app’s answer', async () => {
    open();
    await screen.findByRole('list', { name: 'Log lines' });
    fireEvent.click(screen.getByRole('button', { name: /Problems/ }));
    expect(screen.getAllByRole('listitem')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: 'Details' }));
    expect(screen.getByText('busy, try later')).toBeInTheDocument();
    expect(screen.getByText('162 ms')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /^All/ }));
    fireEvent.change(screen.getByRole('combobox', { name: 'Step' }), { target: { value: 'mail' } });
    expect(screen.getAllByRole('listitem')).toHaveLength(1);
    expect(screen.getByText('Step `mail` succeeded in 12 ms')).toBeInTheDocument();
  });
});
