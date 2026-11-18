import { fireEvent, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Org } from '../../api/types';
import { renderWithProviders } from '../../test/render';
import { SettingsPage } from './SettingsPage';

const baseOrg: Org = { id: 'org_1', name: 'Default org', timezone: 'UTC', retention_days: null, retention_days_effective: 30, retention_days_default: 30 };

function stubApi(org: Org = baseOrg) {
  const patches: unknown[] = [];
  let current = org;
  vi.stubGlobal('fetch', vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
    if (init?.method === 'PATCH') {
      const body = JSON.parse(String(init.body)) as { retention_days: number | null };
      patches.push(body);
      current = { ...current, retention_days: body.retention_days, retention_days_effective: body.retention_days ?? current.retention_days_default };
    }
    return new Response(JSON.stringify(current));
  }));
  return patches;
}

describe('settings: run history', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('starts on the server default and explains the cleanup', async () => {
    stubApi();
    renderWithProviders(<SettingsPage />);
    expect(await screen.findByRole('radio', { name: 'Use server default (30 days)' })).toBeChecked();
    expect(screen.getByText(/cleanup runs every 15 minutes/)).toBeInTheDocument();
    expect(screen.getByText(/Queued, running and waiting runs are never deleted/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('validates 1–3650 days and saves the number', async () => {
    const patches = stubApi();
    renderWithProviders(<SettingsPage />);
    const days = await screen.findByLabelText('Days to keep finished runs');
    fireEvent.change(days, { target: { value: '0' } });
    expect(screen.getByRole('radio', { name: 'Keep finished runs for a number of days' })).toBeChecked();
    expect(screen.getByRole('alert')).toHaveTextContent('Use 1 to 3,650 days.');
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    fireEvent.change(days, { target: { value: '4000' } });
    expect(screen.getByRole('alert')).toHaveTextContent('Use 1 to 3,650 days.');
    fireEvent.change(days, { target: { value: '90' } });
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(patches).toEqual([{ retention_days: 90 }]));
    expect(await screen.findByText(/Now: runs are kept for 90 days\./)).toBeInTheDocument();
  });

  it('asks before keeping runs for fewer days, and saves nothing when cancelled', async () => {
    const patches = stubApi();
    const confirm = vi.fn(() => false);
    vi.stubGlobal('confirm', confirm);
    renderWithProviders(<SettingsPage />);
    fireEvent.change(await screen.findByLabelText('Days to keep finished runs'), { target: { value: '7' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(confirm).toHaveBeenCalledWith(expect.stringMatching(/^Keep runs for 7 days instead of 30\? .*cannot be undone/));
    expect(patches).toEqual([]);
    confirm.mockReturnValue(true);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(patches).toEqual([{ retention_days: 7 }]));
  });

  it('goes back to the server default by sending null', async () => {
    const patches = stubApi({ ...baseOrg, retention_days: 7, retention_days_effective: 7 });
    renderWithProviders(<SettingsPage />);
    expect(await screen.findByLabelText('Days to keep finished runs')).toHaveValue('7');
    fireEvent.click(screen.getByRole('radio', { name: 'Use server default (30 days)' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(patches).toEqual([{ retention_days: null }]));
  });
});
