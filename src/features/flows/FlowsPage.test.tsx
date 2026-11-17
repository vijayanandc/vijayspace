import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Flow, FlowSummary } from '../../api/types';
import { renderWithProviders } from '../../test/render';
import { FlowsPage } from './FlowsPage';

const summary = (over: Partial<FlowSummary>): FlowSummary => ({
  id: 'flw_1',
  name: 'Order alert',
  description: null,
  is_enabled: true,
  trigger_type: 'webhook',
  published_version: 2,
  has_unpublished_changes: true,
  draft_revision: 3,
  updated_at: '2026-01-01T00:00:00Z',
  last_execution: null,
  ...over,
});

const flows = [summary({}), summary({ id: 'flw_2', name: 'Sync deals', is_enabled: false, trigger_type: 'app', published_version: null })];

function detail(id: string): Flow {
  const base = flows.find((f) => f.id === id) ?? summary({ id });
  return {
    ...base,
    draft: { schema_version: 1, trigger: { type: 'webhook' }, steps: [{ key: 'a', type: 'action', connector: 'core.email', action: 'send', input: {} }] },
    published_version_id: null,
    created_at: base.updated_at,
    trigger: { kind: 'webhook', token: 't', is_active: true },
  };
}

function stubApi() {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });
      if (url.endsWith('/api/v1/flows')) return json(flows);
      if (url.includes('/api/v1/flows/')) return json(detail(url.split('/').pop() ?? ''));
      if (url.endsWith('/api/v1/connectors')) return json([]);
      return json({});
    }),
  );
}

describe('FlowsPage', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('lists flows with publish status, app chain and an On/Off switch per row', async () => {
    stubApi();
    renderWithProviders(<FlowsPage />);
    const row = (await screen.findByRole('link', { name: 'Order alert' })).closest('tr');
    expect(row).not.toBeNull();
    if (!row) return;
    expect(within(row).getByText('Published v2')).toBeInTheDocument();
    expect(within(row).getByText('Unpublished changes')).toBeInTheDocument();
    expect(within(row).getByRole('checkbox', { name: 'Disable Order alert' })).toBeChecked();
    await waitFor(() => expect(within(row).getByRole('img', { name: 'core.email' })).toBeInTheDocument());
    expect(screen.getByRole('checkbox', { name: 'Enable Sync deals' })).toBeDisabled();
  });

  it('filters by state and search text, and opens the New flow dialog', async () => {
    stubApi();
    renderWithProviders(<FlowsPage />);
    await screen.findByRole('link', { name: 'Order alert' });
    fireEvent.click(screen.getByRole('button', { name: 'Drafts' }));
    expect(screen.queryByRole('link', { name: 'Order alert' })).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Sync deals' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'All' }));
    fireEvent.change(screen.getByLabelText('Search this list'), { target: { value: 'order' } });
    expect(screen.queryByRole('link', { name: 'Sync deals' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /New flow/ }));
    expect(screen.getByRole('dialog', { name: 'New flow' })).toBeInTheDocument();
    expect(screen.getByLabelText(/Name/)).toHaveFocus();
  });
});
