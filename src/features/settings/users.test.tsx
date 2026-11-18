import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Member, Role } from '../../api/tenancy';
import { renderWithProviders } from '../../test/render';
import { TestSession } from '../auth/session';
import { FlowsPage } from '../flows/FlowsPage';
import { UsersSettings } from './UsersSettings';

const member = (id: string, name: string, role: Role): Member => ({
  org_id: 'org_test', role, joined_at: '2026-11-01T00:00:00Z',
  user: { id, email: `${name.toLowerCase()}@example.com`, email_verified: true, name, created_at: '' },
});

function stubApi() {
  const sent: { method: string; url: string; body?: string }[] = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    sent.push({ method: init?.method ?? 'GET', url, body: init?.body as string | undefined });
    if (url.endsWith('/api/v1/members')) {
      return new Response(JSON.stringify([member('usr_o', 'Olu', 'owner'), member('usr_me', 'Me', 'admin'), member('usr_v', 'Vi', 'viewer')]));
    }
    if (url.endsWith('/api/v1/invitations') && init?.method === 'POST') {
      const body = JSON.parse(String(init.body)) as { email: string; role: Role };
      return new Response(JSON.stringify({ email_sent: true, invitation: { id: 'inv_1', org_id: 'org_test', org_name: 'Test org', email: body.email, role: body.role, invited_by: 'Me', created_at: '', expires_at: '' } }), { status: 201 });
    }
    return new Response('[]');
  }));
  return sent;
}

describe('settings: users', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('lets an admin invite people and change roles, but never touch owners', async () => {
    const sent = stubApi();
    renderWithProviders(<TestSession role="admin"><UsersSettings /></TestSession>);
    const table = await screen.findByRole('table');
    expect(within(table).getByText('Olu')).toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: 'Role of Olu' })).not.toBeInTheDocument();
    const viRole = screen.getByRole('combobox', { name: 'Role of Vi' });
    expect(Array.from((viRole as HTMLSelectElement).options).map((o) => o.value)).toEqual(['viewer', 'admin', 'member']);
    fireEvent.change(viRole, { target: { value: 'member' } });
    expect(sent.some((s) => s.method === 'PATCH')).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Apply role for Vi' }));
    await waitFor(() => expect(sent.some((s) => s.method === 'PATCH' && s.url.endsWith('/api/v1/members/usr_v'))).toBe(true));
    fireEvent.click(screen.getByRole('button', { name: 'Invite people' }));
    const dialog = screen.getByRole('dialog', { name: 'Invite people' });
    expect(within(dialog).queryByRole('radio', { name: /Owner/ })).not.toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText(/Email address/), { target: { value: 'new@example.com' } });
    fireEvent.click(within(dialog).getByRole('radio', { name: /Viewer/ }));
    fireEvent.click(within(dialog).getByRole('button', { name: 'Send invitation' }));
    await waitFor(() => expect(sent.some((s) => s.method === 'POST' && s.url.endsWith('/api/v1/invitations'))).toBe(true));
    const post = sent.find((s) => s.method === 'POST');
    expect(JSON.parse(post?.body ?? '{}')).toEqual({ email: 'new@example.com', role: 'viewer' });
    // The only owner cannot be removed; the admin (me) may leave; the viewer may be removed.
    expect(within(table).getAllByRole('button', { name: /Leave|Remove/ }).map((b) => b.textContent)).toEqual(['Leave', 'Remove']);
    expect(within(within(table).getByText('Olu').closest('tr') as HTMLElement).queryByRole('button')).not.toBeInTheDocument();
  });

  it('shows viewers the team without management, and no "New flow"', async () => {
    stubApi();
    renderWithProviders(
      <TestSession role="viewer">
        <UsersSettings />
        <FlowsPage />
      </TestSession>,
    );
    expect(await screen.findByText('Olu')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Invite people' })).not.toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: /Role of/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /New flow/ })).not.toBeInTheDocument();
  });
});
