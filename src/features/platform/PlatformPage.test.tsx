// The platform page: alerts (critical first, named for screen readers), servers and queue lag; staff only.

import { screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { OpsStatus } from '../../api/ops';
import { renderWithProviders } from '../../test/render';
import { TestSession } from '../auth';
import { PlatformPage } from './PlatformPage';

const status: OpsStatus = {
  now: '2026-11-18T10:00:00Z',
  instances: [
    { id: 'i1', kind: 'api', host: 'web-1', version: '0.1.0', started_at: '2026-11-18T09:00:00Z', seen_at: '2026-11-18T09:59:55Z' },
    { id: 'i2', kind: 'worker', host: 'worker-1', version: '0.1.0', started_at: '2026-11-18T09:30:00Z', seen_at: '2026-11-18T09:59:50Z' },
  ],
  queues: [{ queue: 'executions', ready: 420, lag_seconds: 480 }],
  running_jobs: 16,
  dead_jobs: 1,
  alerts: [
    { key: 'lag:executions', severity: 'warning', title: 'Jobs in `executions` have waited 8 min (420 due)', detail: 'Start another worker.' },
    { key: 'no-worker', severity: 'critical', title: 'No worker is running', detail: 'Start one.' },
  ],
};

describe('platform page', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('shows staff the alerts, servers and how far behind the queue is', async () => {
    const fetchMock = vi.fn(async (_url: RequestInfo | URL) => new Response(JSON.stringify(status)));
    vi.stubGlobal('fetch', fetchMock);
    renderWithProviders(
      <TestSession role="owner" staff>
        <PlatformPage />
      </TestSession>,
    );
    const alerts = await screen.findByRole('heading', { name: 'Alerts · 2' });
    const items = within(alerts.closest('section') as HTMLElement).getAllByRole('listitem');
    expect(items[0]).toHaveTextContent('Critical: No worker is running');
    expect(items[1]).toHaveTextContent('Warning: Jobs in `executions` have waited 8 min');
    expect(screen.getByText('worker-1')).toBeInTheDocument();
    expect(screen.getByText('API')).toBeInTheDocument();
    expect(screen.getByText('8 min')).toBeInTheDocument();
    expect(screen.getByText(/2 servers \(1 worker\) · 2 alerts/)).toBeInTheDocument();
    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/api/v1/ops/status');
  });

  it('tells others it is for staff and asks nothing of the server', () => {
    const fetchMock = vi.fn(async () => new Response('{}'));
    vi.stubGlobal('fetch', fetchMock);
    renderWithProviders(
      <TestSession role="owner">
        <PlatformPage />
      </TestSession>,
    );
    expect(screen.getByText(/Only the operator's staff/)).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
