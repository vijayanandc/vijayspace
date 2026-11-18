import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { Link, MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { qk } from '../../api/hooks';
import type { Role } from '../../api/tenancy';
import type { Flow, FlowDefinition, Step } from '../../api/types';
import { ToastProvider } from '../../components/Toast';
import { TestSession } from '../auth';
import { BuilderPage } from './BuilderPage';

class NoResize {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

/** An in-memory flow API with optimistic concurrency, like the server's. */
function fakeServer(steps: Step[] = []) {
  const server = { revision: 1, draft: { schema_version: 1, trigger: { type: 'manual' }, steps } as FlowDefinition, failGet: false };
  const puts: { revision: number; keys: string[]; status: number }[] = [];
  const flow = (): Flow => ({
    id: 'flw_1', name: 'Demo', description: null, is_enabled: false, trigger_type: 'manual', published_version: null,
    has_unpublished_changes: true, draft_revision: server.revision, updated_at: '', draft: server.draft,
    published_version_id: null, created_at: '', trigger: { kind: 'manual', token: 't', is_active: false },
  });
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    if (url.endsWith('/connectors')) return json([]);
    if (url.endsWith('/validate')) return json({ issues: [] });
    if (url.endsWith('/draft') && method === 'PUT') {
      const body = JSON.parse(String(init?.body)) as { definition: FlowDefinition; revision: number };
      const ok = body.revision === server.revision;
      puts.push({ revision: body.revision, keys: body.definition.steps.map((s) => s.key), status: ok ? 200 : 409 });
      if (!ok) return json({ code: 'flow.revision_conflict', title: 'Conflict' }, 409);
      server.revision += 1;
      server.draft = body.definition;
      return json({ revision: server.revision, issues: [] });
    }
    if (url.endsWith('/flows/flw_1')) return server.failGet ? json({ title: 'Database unavailable' }, 503) : json(flow());
    return json([]);
  }));
  return { server, puts };
}

function renderBuilder(role?: Role) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <MemoryRouter initialEntries={['/flows/flw_1']}>
          <Routes>
            <Route path="/flows" element={<Link to="/flows/flw_1">Open demo</Link>} />
            <Route path="/flows/:flowId" element={role ? <TestSession role={role}><BuilderPage /></TestSession> : <BuilderPage />} />
          </Routes>
        </MemoryRouter>
      </ToastProvider>
    </QueryClientProvider>,
  );
  return { client, ...view };
}

const opened = () => screen.findByRole('button', { name: 'Publish' });
const stepCards = () => document.querySelectorAll('.node-step');

function addLogicStep(label: string) {
  fireEvent.click(screen.getByRole('tab', { name: 'Logic' }));
  const item = [...document.querySelectorAll<HTMLButtonElement>('.palette-item')].find((b) => b.querySelector('.palette-label')?.textContent === label);
  if (!item) throw new Error(`no palette item ${label}`);
  fireEvent.click(item);
}

describe('BuilderPage', () => {
  beforeEach(() => vi.stubGlobal('ResizeObserver', NoResize));
  afterEach(() => vi.unstubAllGlobals());

  it('reopens on the saved draft and keeps saving with the new revision (no conflict)', async () => {
    const { puts } = fakeServer();
    renderBuilder();
    await opened();
    addLogicStep('Stop');
    await waitFor(() => expect(puts).toHaveLength(1), { timeout: 3000 });
    await screen.findByText('Saved');

    fireEvent.click(screen.getByRole('link', { name: 'Back to flows' }));
    fireEvent.click(await screen.findByText('Open demo'));
    await opened();
    expect(stepCards()).toHaveLength(1);

    // An edit made just before leaving is saved on the way out; reopening waits for that save.
    addLogicStep('Delay');
    fireEvent.click(screen.getByRole('link', { name: 'Back to flows' }));
    fireEvent.click(await screen.findByText('Open demo'));
    await opened();
    expect(stepCards()).toHaveLength(2);
    expect(puts.map((p) => [p.revision, p.status])).toEqual([[1, 200], [2, 200]]);
    expect(screen.queryByText(/changed somewhere else/)).not.toBeInTheDocument();
  }, 10_000);

  it('keeps the builder open when a background refresh fails', async () => {
    const { server } = fakeServer();
    const { client } = renderBuilder();
    await opened();
    server.failGet = true;
    await act(() => client.invalidateQueries({ queryKey: qk.flow('flw_1') }));
    expect(await screen.findByText(/Could not refresh this flow/)).toHaveTextContent('Database unavailable');
    expect(screen.getByRole('button', { name: 'Publish' })).toBeInTheDocument();
    server.failGet = false;
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(screen.queryByText(/Could not refresh this flow/)).not.toBeInTheDocument());
  });

  it('shows the error page when the flow cannot be loaded at all', async () => {
    const { server } = fakeServer();
    server.failGet = true;
    renderBuilder();
    expect(await screen.findByRole('alert')).toHaveTextContent('Database unavailable');
  });

  it('keeps the renamed step selected when the rename is undone', async () => {
    fakeServer([{ key: 'wait', type: 'delay', seconds: 60 }]);
    renderBuilder();
    await opened();
    const card = document.querySelector('[data-id="step:wait"]');
    if (!card) throw new Error('no card');
    fireEvent.click(card);
    fireEvent.click(screen.getByRole('tab', { name: 'Settings' }));
    const keyInput = screen.getByLabelText(/Step key/);
    fireEvent.change(keyInput, { target: { value: 'pause' } });
    fireEvent.blur(keyInput);
    expect(document.querySelector('[data-id="step:pause"]')).not.toBeNull();

    fireEvent.keyDown(document.body, { key: 'z', ctrlKey: true });
    expect(document.querySelector('[data-id="step:wait"]')).not.toBeNull();
    const panel = screen.getByRole('complementary', { name: 'Step settings' });
    fireEvent.click(within(panel).getByRole('tab', { name: 'Settings' }));
    expect(within(panel).getByLabelText(/Step key/)).toHaveValue('wait');
  });

  it('shows a viewer the flow without any way to change it', async () => {
    const { puts } = fakeServer([{ key: 'wait', type: 'delay', seconds: 60 }]);
    renderBuilder('viewer');
    expect(await screen.findByText(/View only/)).toBeInTheDocument();
    expect(document.querySelector('.palette')).toBeNull();
    expect(document.querySelector('.insert-node[role="button"]')).toBeNull();
    expect(document.querySelector('.node-delete')).toBeNull();
    expect(stepCards()[0]).not.toHaveAttribute('draggable', 'true');
    fireEvent.click(stepCards()[0] as HTMLElement);
    expect(await screen.findByRole('textbox', { name: 'Step name' })).toHaveAttribute('readonly');
    const panel = screen.getByRole('complementary', { name: 'Step settings' });
    expect(panel.querySelector('fieldset')).toBeDisabled();
    expect(puts).toHaveLength(0);
  });

  it('moves a step with the keyboard: Move to…, then a + on the canvas', async () => {
    fakeServer([
      { key: 'first', type: 'delay', seconds: 1 },
      { key: 'second', type: 'delay', seconds: 2 },
    ]);
    renderBuilder();
    await opened();
    fireEvent.click(stepCards()[1] as HTMLElement);
    fireEvent.click(await screen.findByRole('tab', { name: 'Settings' }));
    fireEvent.click(screen.getByRole('button', { name: 'Move to…' }));
    expect(screen.getByText(/Moving “second”/)).toBeInTheDocument();
    const firstPlus = document.querySelector<HTMLElement>('.insert-node[aria-label="Insert a step at position 1"]');
    if (!firstPlus) throw new Error('no insertion point');
    expect(firstPlus).toHaveFocus();
    fireEvent.keyDown(firstPlus, { key: 'Enter' });
    await waitFor(() => expect([...stepCards()].map((c) => c.textContent)).toEqual([expect.stringContaining('Wait 2 s'), expect.stringContaining('Wait 1 s')]));
    expect(screen.queryByText(/Moving “second”/)).not.toBeInTheDocument();
  });
});

