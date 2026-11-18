import { fireEvent, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { configureAuth, setCurrentOrg } from '../../api/client';
import type { Me, Membership } from '../../api/tenancy';
import { renderWithProviders } from '../../test/render';
import { OrgSwitcher } from '../shell/AccountMenus';
import { AuthRoot } from './AuthRoot';

const logto = vi.hoisted(() => ({
  isAuthenticated: false,
  isLoading: false,
  signIn: vi.fn(async () => undefined),
  signOut: vi.fn(async () => undefined),
  getAccessToken: vi.fn(async () => 'token-1'),
  clearAccessToken: vi.fn(async () => undefined),
  clearAllTokens: vi.fn(async () => undefined),
}));

vi.mock('@logto/react', () => ({
  LogtoProvider: ({ children }: { children: unknown }) => children,
  useLogto: () => logto,
  useHandleSignInCallback: () => ({ isAuthenticated: logto.isAuthenticated, error: undefined }),
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
    expect(screen.getByRole('menuitemradio', { name: /Beta/ })).toHaveAttribute('aria-checked', 'true');
    fireEvent.click(screen.getByRole('menuitemradio', { name: /Alpha/ }));
    expect(window.localStorage.getItem('weft.org')).toBe('org_a');
    await waitFor(() => expect(calls.list.at(-1)?.headers['X-Org-ID']).toBe('org_a'));
  });
});
