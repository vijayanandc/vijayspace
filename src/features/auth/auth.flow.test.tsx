import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider, useMutation } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { StrictMode } from 'react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { api, configureAuth, request, setCurrentOrg } from '../../api/client';
import { mk } from '../../api/hooks';
import type { Me, Membership } from '../../api/tenancy';
import { renderWithProviders } from '../../test/render';
import { OrgSwitcher } from '../shell/AccountMenus';
import { AuthRoot } from './AuthRoot';
import { useSession } from './session';

const logto = vi.hoisted(() => ({
  isAuthenticated: false,
  isLoading: false,
  error: undefined as Error | undefined,
  signIn: vi.fn(async () => undefined),
  signOut: vi.fn(async () => undefined),
  getAccessToken: vi.fn(async () => 'token-1'),
  clearAccessToken: vi.fn(async () => undefined),
  clearAllTokens: vi.fn(async () => undefined),
}));

vi.mock('@logto/react', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@logto/react')>()),
  LogtoProvider: ({ children }: { children: unknown }) => children,
  useLogto: () => logto,
  useHandleSignInCallback: () => ({ isAuthenticated: logto.isAuthenticated, isLoading: false, error: undefined }),
}));

const org = (id: string, name: string, role: Membership['role'] = 'owner'): Membership => ({
  id, name, role, timezone: 'UTC', retention_days: null, retention_days_effective: 30, retention_days_default: 30,
});

interface Calls {
  list: { method: string; url: string; headers: Record<string, string>; body?: string }[];
}

function stubApi(signIn: boolean, me: Me): Calls {
  const calls: Calls = { list: [] };
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    calls.list.push({ method: init?.method ?? 'GET', url, headers: (init?.headers ?? {}) as Record<string, string>, body: init?.body as string | undefined });
    if (url.endsWith('/api/v1/auth/config')) {
      return new Response(JSON.stringify({ sign_in: signIn ? { endpoint: 'https://t.logto.app', app_id: 'web', resource: null, scopes: ['openid'] } : null }));
    }
    if (url.endsWith('/api/v1/me')) return new Response(JSON.stringify(me));
    if (url.endsWith('/api/v1/orgs') && init?.method === 'POST') return new Response(JSON.stringify(org('org_new', 'Acme')), { status: 201 });
    return new Response(JSON.stringify([]));
  }));
  return calls;
}

const user = { id: 'usr_1', email: 'ann@example.com', email_verified: true, name: 'Ann', created_at: '' };

describe('sign-in', () => {
  beforeEach(() => {
    logto.isAuthenticated = false;
    logto.isLoading = false;
    logto.error = undefined;
    window.localStorage.clear();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    configureAuth(null);
    setCurrentOrg(null);
  });

  it('opens the app as a local owner when the server has sign-in turned off', async () => {
    stubApi(false, { user: null, orgs: [org('org_default', 'Default org')], invitations: [] });
    renderWithProviders(<AuthRoot><p>the app</p></AuthRoot>);
    expect(await screen.findByText('the app')).toBeInTheDocument();
    expect(logto.signIn).not.toHaveBeenCalled();
  });

  it('shows the sign-in page when signed out, and sends people to Logto with this origin’s callback', async () => {
    stubApi(true, { user, orgs: [], invitations: [] });
    renderWithProviders(<AuthRoot><p>the app</p></AuthRoot>);
    fireEvent.click(await screen.findByRole('button', { name: /Sign in/ }));
    expect(logto.signIn).toHaveBeenCalledWith(`${window.location.origin}/callback`);
    expect(screen.queryByText('the app')).not.toBeInTheDocument();
  });

  it('signs API calls in with the access token, and onboards a user without an org', async () => {
    logto.isAuthenticated = true;
    const calls = stubApi(true, { user, orgs: [], invitations: [] });
    renderWithProviders(<AuthRoot><p>the app</p></AuthRoot>);
    expect(await screen.findByRole('heading', { name: 'Welcome, Ann' })).toBeInTheDocument();
    const me = calls.list.find((c) => c.url.endsWith('/api/v1/me'));
    expect(me?.headers.Authorization).toBe('Bearer token-1');
    fireEvent.change(screen.getByLabelText(/Organisation name/), { target: { value: 'Acme' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create organisation' }));
    await waitFor(() => expect(calls.list.some((c) => c.method === 'POST' && c.url.endsWith('/api/v1/orgs'))).toBe(true));
    const post = calls.list.find((c) => c.method === 'POST');
    expect(JSON.parse(post?.body ?? '{}')).toMatchObject({ name: 'Acme' });
  });

  it('acts in the remembered org and switches orgs from the header', async () => {
    logto.isAuthenticated = true;
    window.localStorage.setItem('weft.org', 'org_b');
    const calls = stubApi(true, { user, orgs: [org('org_a', 'Alpha'), org('org_b', 'Beta', 'viewer')], invitations: [] });
    renderWithProviders(<AuthRoot><OrgSwitcher /></AuthRoot>);
    fireEvent.click(await screen.findByRole('button', { name: /Beta/ }));
    const group = screen.getByRole('group', { name: 'Organisations' });
    expect(within(group).getByRole('button', { name: /Beta/ })).toHaveAttribute('aria-current', 'true');
    fireEvent.click(within(group).getByRole('button', { name: /Alpha/ }));
    await waitFor(() => expect(window.localStorage.getItem('weft.org')).toBe('org_a'));
    await waitFor(() => expect(calls.list.at(-1)?.headers['X-Org-ID']).toBe('org_a'));
  });

  it('lets people who already have an org accept an invitation from the switcher', async () => {
    logto.isAuthenticated = true;
    const invitation = { id: 'inv_1', org_id: 'org_c', org_name: 'Gamma', email: 'ann@example.com', role: 'member' as const, invited_by: 'Bo', created_at: '', expires_at: '2026-12-01T00:00:00Z' };
    const calls = stubApi(true, { user, orgs: [org('org_a', 'Alpha')], invitations: [invitation] });
    vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
      calls.list.push({ method: init?.method ?? 'GET', url, headers: (init?.headers ?? {}) as Record<string, string> });
      if (url.endsWith('/accept')) return new Response(JSON.stringify(org('org_c', 'Gamma', 'member')));
      if (url.endsWith('/api/v1/auth/config')) return new Response(JSON.stringify({ sign_in: { endpoint: 'https://t', app_id: 'web', resource: null, scopes: [] } }));
      if (url.endsWith('/api/v1/me')) return new Response(JSON.stringify({ user, orgs: [org('org_a', 'Alpha')], invitations: [invitation] }));
      return new Response('[]');
    }));
    renderWithProviders(<AuthRoot><OrgSwitcher /></AuthRoot>);
    fireEvent.click(await screen.findByRole('button', { name: /Alpha/ }));
    expect(screen.getByRole('button', { name: /Alpha.*1 invitation waiting/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Accept Gamma' }));
    await waitFor(() => expect(window.localStorage.getItem('weft.org')).toBe('org_c'));
  });
});

describe('signing out', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
    configureAuth(null);
    logto.error = undefined;
  });

  it('lets a draft save finish signed in, then sends nothing unsigned', async () => {
    logto.isAuthenticated = true;
    const calls = stubApi(true, { user, orgs: [org('org_a', 'Alpha')], invitations: [] });
    let release = () => undefined as void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    renderWithProviders(<AuthRoot><SaveThenSignOut gate={gate} /></AuthRoot>);
    fireEvent.click(await screen.findByRole('button', { name: 'Save' }));
    fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
    expect(await screen.findByText('Signing you out…')).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 20));
    expect(logto.signOut).not.toHaveBeenCalled();
    release();
    await waitFor(() => expect(logto.signOut).toHaveBeenCalled());
    const save = calls.list.find((c) => c.url.endsWith('/flows/flw_1/draft'));
    expect(save?.headers.Authorization).toBe('Bearer token-1');
    const after = calls.list.length;
    await expect(request('GET', '/api/v1/me')).rejects.toMatchObject({ code: 'auth.unavailable' });
    expect(calls.list).toHaveLength(after);
  });

  it('keeps the button busy when an old sign-in error is still showing', async () => {
    logto.isAuthenticated = false;
    logto.error = new Error('an earlier failure');
    stubApi(true, { user, orgs: [], invitations: [] });
    renderWithProviders(<AuthRoot><p>the app</p></AuthRoot>);
    fireEvent.click(await screen.findByRole('button', { name: 'Sign in' }));
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.getByRole('button', { name: 'Opening sign-in…' })).toBeDisabled();
  });
});

function SaveThenSignOut({ gate }: { gate: Promise<void> }) {
  const session = useSession();
  const save = useMutation({
    mutationKey: mk.saveDraft('flw_1'),
    mutationFn: async () => {
      await gate;
      return api.put('/api/v1/flows/flw_1/draft', {});
    },
  });
  return (
    <>
      <button type="button" onClick={() => save.mutate()}>Save</button>
      <button type="button" onClick={() => session?.signOut()}>Sign out</button>
    </>
  );
}

describe('ending a session', () => {
  it('treats a refresh Logto rejects as signed out, and a network failure as not', async () => {
    const { LogtoClientError, LogtoRequestError } = await vi.importActual<typeof import('@logto/react')>('@logto/react');
    const { sessionEnded } = await import('./LogtoGate');
    expect(sessionEnded(new LogtoRequestError('oidc.invalid_grant', 'grant request is invalid'))).toBe(true);
    expect(sessionEnded(new LogtoClientError('not_authenticated'))).toBe(true);
    expect(sessionEnded(new TypeError('Failed to fetch'))).toBe(false);
    expect(sessionEnded(new LogtoRequestError('oidc.server_error', 'down'))).toBe(false);
    expect(sessionEnded(undefined)).toBe(false);
  });
});

describe('returning after sign-in', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    configureAuth(null);
    window.sessionStorage.clear();
  });

  it('opens the page the user started on, also under StrictMode', async () => {
    logto.isAuthenticated = true;
    window.sessionStorage.setItem('weft.return', '/settings?tab=users');
    stubApi(true, { user, orgs: [org('org_a', 'Alpha')], invitations: [] });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <StrictMode>
        <QueryClientProvider client={client}>
          <MemoryRouter initialEntries={['/callback?code=x&state=y']}>
            <AuthRoot>
              <Where />
            </AuthRoot>
          </MemoryRouter>
        </QueryClientProvider>
      </StrictMode>,
    );
    expect(await screen.findByText('at /settings?tab=users')).toBeInTheDocument();
    expect(window.sessionStorage.getItem('weft.return')).toBeNull();
  });
});

function Where() {
  const { pathname, search } = useLocation();
  return <p>{`at ${pathname}${search}`}</p>;
}
