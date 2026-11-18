import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Connection, ConnectorManifest } from '../../api/types';
import { ToastProvider } from '../../components/Toast';
import { renderWithProviders } from '../../test/render';
import { ConnectionCallbackPage } from './ConnectionCallbackPage';
import { ConnectionDialog } from './ConnectionDialog';
import { ConnectionsPage } from './ConnectionsPage';

const crm: ConnectorManifest = {
  key: 'zoho_crm', name: 'Zoho CRM', description: '', icon: '', category: '',
  auth: {
    kind: 'oauth2',
    fields: [
      { key: 'region', label: 'Region', type: 'select', options: [{ value: 'in', label: 'India' }] },
      { key: 'client_secret', label: 'Secret', type: 'password', secret: true },
    ],
  },
  actions: [], triggers: [],
};

const stale: Connection = {
  id: 'cxn_1', connector: 'zoho_crm', name: 'Sales CRM', config: { region: 'in', api_domain: 'x' }, status: 'needs_reauth', created_at: '', updated_at: '',
};

/** Stubs the API; returns the bodies posted to `oauth/start`. The authorize URL is a hash so jsdom can "navigate". */
function stubApi() {
  const starts: Record<string, unknown>[] = [];
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith('/connectors')) return new Response(JSON.stringify([crm]));
    if (url.endsWith('/connections')) return new Response(JSON.stringify([stale]));
    if (url.endsWith('/connections/usage')) return new Response(JSON.stringify([]));
    if (url.endsWith('/oauth/start')) {
      starts.push(JSON.parse(String(init?.body)) as Record<string, unknown>);
      return new Response(JSON.stringify({ authorize_url: '#signin' }));
    }
    return new Response('{}');
  }));
  return starts;
}

describe('OAuth connections', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('saves the flow before leaving for sign-in from the builder, and comes back to the flow', async () => {
    const starts = stubApi();
    const order: string[] = [];
    const flush = vi.fn(async () => {
      order.push('flush');
      return true;
    });
    renderWithProviders(<ConnectionDialog open onClose={vi.fn()} connector="zoho_crm" returnTo="/flows/flw_1" beforeRedirect={flush} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Connect to Zoho CRM' }));
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(flush).toHaveBeenCalledTimes(1);
    expect(starts[0]?.return_to).toBe(`${window.location.origin}/connections/callback?next=%2Fflows%2Fflw_1`);
  });

  it('does not leave for sign-in when the flow could not be saved', async () => {
    const starts = stubApi();
    renderWithProviders(<ConnectionDialog open onClose={vi.fn()} connector="zoho_crm" returnTo="/flows/flw_1" beforeRedirect={async () => false} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Connect to Zoho CRM' }));
    expect(await screen.findByText(/could not be saved, so sign-in was not started/)).toBeInTheDocument();
    expect(starts).toHaveLength(0);
  });

  it('returns to the flow after a successful sign-in', async () => {
    stubApi();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ToastProvider>
          <MemoryRouter initialEntries={['/connections/callback?next=%2Fflows%2Fflw_1&status=connected&connection_id=cxn_9']} future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
            <Routes>
              <Route path="/connections/callback" element={<ConnectionCallbackPage />} />
              <Route path="/flows/:flowId" element={<p>Builder</p>} />
            </Routes>
          </MemoryRouter>
        </ToastProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText('Builder')).toBeInTheDocument();
    expect(screen.getByText(/Connected\. Choose the new connection/)).toBeInTheDocument();
  });

  it('re-authorises a connection that needs it, keeping its settings but no secrets', async () => {
    const starts = stubApi();
    renderWithProviders(<ConnectionsPage />);
    fireEvent.click(await screen.findByRole('button', { name: 'Reconnect' }));
    await waitFor(() => expect(starts).toHaveLength(1));
    expect(starts[0]).toMatchObject({ connector: 'zoho_crm', name: 'Sales CRM', connection_id: 'cxn_1', fields: { region: 'in' } });
    expect(starts[0]?.fields).toEqual({ region: 'in' });
  });
});
