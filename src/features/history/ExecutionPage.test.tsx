import { fireEvent, screen } from '@testing-library/react';
import { render } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ExecutionDetail } from '../../api/types';
import { ToastProvider } from '../../components/Toast';
import { TestSession } from '../auth';
import { ExecutionPage } from './ExecutionPage';

const execution = (hidden: boolean): ExecutionDetail => ({
  id: 'exe_1', flow_id: 'flw_1', flow_name: 'Orders', status: 'succeeded', mode: 'live', queued_at: '2026-11-18T10:00:00Z',
  started_at: '2026-11-18T10:00:01Z', finished_at: '2026-11-18T10:00:02Z', waiting_until: null, task_count: 1, error_code: null,
  error_message: null, failed_step_key: null, possible_duplicate: false, trigger_data: hidden ? null : { body: { id: 7 } }, steps: [],
  payloads_hidden: hidden,
});

function open(hidden: boolean, path = '/history/exe_1') {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: RequestInfo | URL) =>
      String(url).endsWith('/log')
        ? new Response(JSON.stringify({ truncated: false, entries: [{ at: '2026-11-18T10:00:00Z', level: 'info', kind: 'run', step_key: null, attempt: null, message: 'Run queued (live)', details: null }] }))
        : new Response(JSON.stringify(execution(hidden))),
    ),
  );
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <ToastProvider>
        <TestSession role={hidden ? 'viewer' : 'owner'}>
          <MemoryRouter initialEntries={[path]}>
            <Routes>
              <Route path="/history/:executionId" element={<ExecutionPage />} />
            </Routes>
          </MemoryRouter>
        </TestSession>
      </ToastProvider>
    </QueryClientProvider>,
  );
}

describe('execution page', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('tells viewers their run data is hidden instead of showing an empty payload', async () => {
    open(true);
    expect(await screen.findByText(/Run data is hidden for viewers/)).toBeInTheDocument();
    expect(screen.getByText('Hidden for viewers.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Payload/ })).not.toBeInTheDocument();
  });

  it('opens the run log from its tab or a ?view=log link', async () => {
    open(false, '/history/exe_1?view=log');
    expect(await screen.findByText('Run queued (live)')).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Log' })).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(screen.getByRole('tab', { name: /Steps/ }));
    expect(await screen.findByRole('button', { name: /^Payload/ })).toBeInTheDocument();
  });

  it('shows the trigger data to others', async () => {
    open(false);
    expect(await screen.findByRole('button', { name: /^Payload/ })).toBeInTheDocument();
    expect(screen.queryByText(/Run data is hidden/)).not.toBeInTheDocument();
  });
});
