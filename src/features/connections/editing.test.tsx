// Saving and editing connections: a failed test keeps the dialog open with the app's reason (and saving again
// updates the same connection), and an edit sends only what changed.

import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Connection, ConnectorManifest } from '../../api/types';
import { renderWithProviders } from '../../test/render';
import { ConnectionDialog } from './ConnectionDialog';
import { ConnectionsPage } from './ConnectionsPage';

const fsm: ConnectorManifest = {
  key: 'appblocks', name: 'Appblocks', description: '', icon: '', category: '',
  auth: {
    kind: 'fields',
    fields: [
      { key: 'base_url', label: 'App URL', type: 'string', required: true },
      { key: 'api_token', label: 'API token', type: 'password', secret: true, required: true },
    ],
  },
  actions: [], triggers: [],
};

const broken: Connection = { id: 'cxn_9', connector: 'appblocks', name: 'FSM', config: { base_url: 'https://fsm.example.com' }, status: 'error', created_at: '', updated_at: '' };

/** Stubs the API; the connection test fails until `fixed`. Returns the writes (method, path, body). */
function stubApi(opts: { fixed?: () => boolean } = {}) {
  const writes: { method: string; path: string; body: unknown }[] = [];
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = new URL(String(input), 'http://x').pathname;
    const method = init?.method ?? 'GET';
    if (method !== 'GET') writes.push({ method, path, body: init?.body ? JSON.parse(String(init.body)) : null });
    if (path.endsWith('/connectors')) return new Response(JSON.stringify([fsm]));
    if (path.endsWith('/connections/usage')) return new Response('[]');
    if (path.endsWith('/connections') && method === 'GET') return new Response(JSON.stringify([broken]));
    if (path.endsWith('/test')) {
      const ok = opts.fixed?.() ?? false;
      return new Response(JSON.stringify(ok ? { ok: true } : { ok: false, error: { code: 'app.auth', message: 'organization not found.' } }));
    }
    if (path.endsWith('/connections') || path.endsWith('/connections/cxn_9')) return new Response(JSON.stringify({ ...broken, status: 'error' }));
    return new Response('{}');
  }));
  return writes;
}

describe('saving a connection', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('keeps the dialog open with the reason when the test fails, and saving again updates the same one', async () => {
    let fixed = false;
    const writes = stubApi({ fixed: () => fixed });
    const onClose = vi.fn();
    renderWithProviders(<ConnectionDialog open onClose={onClose} connector="appblocks" />);
    fireEvent.change(await screen.findByLabelText(/App URL/), { target: { value: 'https://fsm.example.com' } });
    fireEvent.change(screen.getByLabelText(/API token/), { target: { value: 'bogus' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save and test' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('organization not found. Fix the details');
    expect(onClose).not.toHaveBeenCalled();

    fixed = true;
    fireEvent.change(screen.getByLabelText(/API token/), { target: { value: 'good' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save and test' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(writes.map((w) => `${w.method} ${w.path}`)).toEqual([
      'POST /api/v1/connections',
      'POST /api/v1/connections/cxn_9/test',
      'PATCH /api/v1/connections/cxn_9',
      'POST /api/v1/connections/cxn_9/test',
    ]);
  });

  it('edits a connection from its row: a new token alone is sent, then tested', async () => {
    const writes = stubApi({ fixed: () => true });
    renderWithProviders(<ConnectionsPage />);
    fireEvent.click(await screen.findByRole('button', { name: 'More actions for FSM' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Edit' }));
    const dialog = await screen.findByRole('dialog', { name: 'Edit connection' });
    expect(within(dialog).getByLabelText(/App URL/)).toHaveValue('https://fsm.example.com');
    expect(within(dialog).getByLabelText(/API token/)).toHaveValue('');
    fireEvent.change(within(dialog).getByLabelText(/API token/), { target: { value: 'rotated' } });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save and test' }));
    await waitFor(() => expect(writes).toHaveLength(2));
    expect(writes[0]).toEqual({ method: 'PATCH', path: '/api/v1/connections/cxn_9', body: { name: 'FSM', fields: { api_token: 'rotated' } } });
    expect(writes[1]?.path).toBe('/api/v1/connections/cxn_9/test');
  });

  it('asks for the token again before a changed address can be saved', async () => {
    stubApi();
    renderWithProviders(<ConnectionsPage />);
    fireEvent.click(await screen.findByRole('button', { name: 'More actions for FSM' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Edit' }));
    const dialog = await screen.findByRole('dialog', { name: 'Edit connection' });
    fireEvent.change(within(dialog).getByLabelText(/App URL/), { target: { value: 'https://other.example.com' } });
    expect(within(dialog).getByText(/needs the credentials again: enter API token/)).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Save and test' })).toBeDisabled();
  });
});
