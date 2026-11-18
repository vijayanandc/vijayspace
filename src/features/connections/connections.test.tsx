import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { StrictMode } from 'react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { setCurrentOrg } from '../../api/client';
import type { Connection, ConnectorManifest } from '../../api/types';
import { TestSession } from '../auth';
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
          <MemoryRouter initialEntries={['/connections/callback?next=%2Fflows%2Fflw_1&status=connected&connection_id=cxn_9']}>
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

  describe('finishing a sign-in the app sent back', () => {
    afterEach(() => setCurrentOrg(null));

    function finishPage(entry: string) {
      render(
        <StrictMode>
          <QueryClientProvider client={new QueryClient()}>
            <ToastProvider>
              <TestSession role="member">
                <MemoryRouter initialEntries={[entry]}>
                  <Routes>
                    <Route path="/connections/callback" element={<><ConnectionCallbackPage /><Address /></>} />
                    <Route path="/flows/:flowId" element={<p>Builder</p>} />
                  </Routes>
                </MemoryRouter>
              </TestSession>
            </ToastProvider>
          </QueryClientProvider>
        </StrictMode>,
      );
    }

    function stubFinish(answer: () => Response) {
      const finishes: { body: unknown; org: string | undefined }[] = [];
      vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        if (String(input).endsWith('/oauth/finish')) {
          finishes.push({ body: JSON.parse(String(init?.body)), org: (init?.headers as Record<string, string>)['X-Org-ID'] });
          return answer();
        }
        return new Response('[]');
      }));
      return finishes;
    }

    it('finishes once, in the org it began in, then returns to the flow', async () => {
      setCurrentOrg('org_other');
      const finishes = stubFinish(() => new Response(JSON.stringify({ connection_id: 'cxn_7' })));
      finishPage('/connections/callback?next=%2Fflows%2Fflw_1&org=org_test&status=finish&state=st1&finish=fin1');
      expect(await screen.findByText('Builder')).toBeInTheDocument();
      expect(finishes).toEqual([{ body: { state: 'st1', finish: 'fin1' }, org: 'org_test' }]);
    });

    it('shows why it failed and drops the finish token from the address', async () => {
      setCurrentOrg('org_test');
      const finishes = stubFinish(() => new Response(JSON.stringify({ code: 'oauth.state_invalid', title: 'Invalid', detail: 'This sign-in was started by someone else; start again' }), { status: 422 }));
      finishPage('/connections/callback?org=org_elsewhere&status=finish&state=st1&finish=fin1');
      expect(await screen.findByRole('alert')).toHaveTextContent('started by someone else');
      expect(finishes).toHaveLength(1);
      expect(finishes[0]?.org).toBe('org_test');
      expect(screen.getByTestId('address').textContent).not.toContain('fin1');
    });
  });
});

function Address() {
  const { search } = useLocation();
  return <output data-testid="address">{search}</output>;
}
