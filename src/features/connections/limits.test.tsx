import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Connection, ConnectionUsage, ConnectorManifest, LimitsView } from '../../api/types';
import { renderWithProviders } from '../../test/render';
import { TestSession } from '../auth/session';
import { ConnectionsPage } from './ConnectionsPage';
import { UsageMeter } from './UsageMeter';

const appblocks: ConnectorManifest = {
  key: 'appblocks', name: 'Appblocks', description: '', icon: '', category: '', actions: [], triggers: [],
  auth: { kind: 'fields', fields: [{ key: 'base_url', label: 'Base URL', type: 'string' }] },
};
const conn: Connection = { id: 'cxn_1', connector: 'appblocks', name: 'Acme FSM', config: {}, status: 'active', created_at: '', updated_at: '' };
const usage: ConnectionUsage = { connection_id: 'cxn_1', day: '2026-11-18', requests: 8500, refused: 3, daily_limit: 10000, percent: 85, level: 'warning' };
const view: LimitsView = {
  defaults: { requests_per_minute: 300, max_concurrent: 10, daily_requests: 10000 },
  overrides: { requests_per_minute: 60, max_concurrent: null, daily_requests: null },
  effective: { requests_per_minute: 60, max_concurrent: 10, daily_requests: 10000 },
  usage,
};

function stubApi() {
  const puts: unknown[] = [];
  let usageCalls = 0;
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
    if (url.endsWith('/connectors')) return json([appblocks]);
    if (url.endsWith('/connections')) return json([conn]);
    if (url.endsWith('/connections/usage')) {
      usageCalls += 1;
      return json([usage]);
    }
    if (url.endsWith('/connections/cxn_1/limits') && init?.method === 'PUT') {
      const body = JSON.parse(String(init.body)) as LimitsView['overrides'];
      puts.push(body);
      return json({ ...view, overrides: body });
    }
    if (url.endsWith('/connections/cxn_1/limits')) return json(view);
    return json({});
  }));
  return { puts, usageCalls: () => usageCalls };
}

describe('connection limits and usage', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('shows the limits read-only to someone who may not change this connection', async () => {
    const api = stubApi();
    renderWithProviders(<TestSession role="viewer"><ConnectionsPage /></TestSession>);
    fireEvent.click(await screen.findByRole('button', { name: /Limits and usage of Acme FSM/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Limits & usage' });
    const perMinute = await within(dialog).findByLabelText('Requests per minute (custom value)');
    expect(perMinute).toBeDisabled();
    expect(within(dialog).getByRole('radio', { name: 'Use app default (10,000)' })).toBeDisabled();
    expect(within(dialog).queryByRole('button', { name: 'Save limits' })).not.toBeInTheDocument();
    expect(api.puts).toHaveLength(0);
  });

  it('shows today’s usage with its level and refused requests in the table', async () => {
    stubApi();
    renderWithProviders(<ConnectionsPage />);
    const cell = await screen.findByRole('button', { name: 'Limits and usage of Acme FSM: 8,500 of 10,000 requests used today (85%), 3 refused' });
    await within(cell).findByText('8,500 / 10,000');
    expect(within(cell).getByText('3 refused')).toBeInTheDocument();
    expect(cell.querySelector('.usage')).toHaveClass('usage-warning');
    // A button's content is presentational: no meter inside it (the name carries the numbers).
    expect(within(cell).queryByRole('meter')).not.toBeInTheDocument();
  });

  it('keeps the dialog meter within its range when inbound events go past the budget', () => {
    const over: ConnectionUsage = { connection_id: 'con_1', day: '2026-11-18', requests: 10_250, refused: 0, daily_limit: 10_000, percent: 102.5, level: 'critical' };
    renderWithProviders(<UsageMeter usage={over} detailed />);
    const meter = screen.getByRole('meter', { name: 'Daily requests used' });
    expect(meter).toHaveAttribute('aria-valuenow', '10000');
    expect(meter).toHaveAttribute('aria-valuemax', '10000');
    expect(meter).toHaveAttribute('aria-valuetext', '10,250 of 10,000 requests used today (102%)');
  });

  it('edits the limits: app default, no limit and a validated custom value', async () => {
    const api = stubApi();
    renderWithProviders(<ConnectionsPage />);
    fireEvent.click(await screen.findByRole('button', { name: 'More actions for Acme FSM' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Limits & usage' }));
    const dialog = await screen.findByRole('dialog', { name: 'Limits & usage' });
    const perMinute = await within(dialog).findByLabelText('Requests per minute (custom value)');
    expect(within(dialog).getByText(/Requests over a limit are retried later, never dropped/)).toBeInTheDocument();
    expect(within(dialog).getByText(/pauses at 95% of the daily budget/)).toBeInTheDocument();
    expect(within(dialog).getByRole('region', { name: 'Today’s usage' })).toHaveTextContent('8,500 / 10,000');
    expect(perMinute).toHaveValue('60');
    expect(within(dialog).getByRole('radio', { name: 'Use app default (10,000)' })).toBeChecked();
    expect(within(dialog).getByLabelText('Max concurrent requests (custom value)')).toHaveAttribute('placeholder', '10');

    fireEvent.change(perMinute, { target: { value: '12.5' } });
    expect(within(dialog).getByText('Use a whole number.')).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save limits' }));
    expect(api.puts).toHaveLength(0);

    fireEvent.change(perMinute, { target: { value: '120' } });
    const concurrent = within(dialog).getByRole('radiogroup', { name: 'Max concurrent requests setting' });
    fireEvent.click(within(concurrent).getByRole('radio', { name: 'No limit' }));
    const before = api.usageCalls();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save limits' }));
    await waitFor(() => expect(api.puts).toEqual([{ requests_per_minute: 120, max_concurrent: 0, daily_requests: null }]));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    await waitFor(() => expect(api.usageCalls()).toBeGreaterThan(before));
  });

  it('opens the dialog from the usage cell and sends null for "Use app default"', async () => {
    const api = stubApi();
    renderWithProviders(<ConnectionsPage />);
    fireEvent.click(await screen.findByRole('button', { name: /^Limits and usage of Acme FSM: / }));
    const dialog = await screen.findByRole('dialog', { name: 'Limits & usage' });
    const perMinute = await within(dialog).findByRole('radiogroup', { name: 'Requests per minute setting' });
    fireEvent.click(await within(perMinute).findByRole('radio', { name: 'Use app default (300)' }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save limits' }));
    await waitFor(() => expect(api.puts).toEqual([{ requests_per_minute: null, max_concurrent: null, daily_requests: null }]));
  });
});
