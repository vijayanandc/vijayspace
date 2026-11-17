import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { type ReactNode, createElement } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { qk } from '../../api/hooks';
import type { Flow, FlowDefinition } from '../../api/types';
import { useAutosave } from './useAutosave';

const d0: FlowDefinition = { schema_version: 1, trigger: { type: 'manual' }, steps: [] };
const d1: FlowDefinition = { ...d0, steps: [{ key: 'a', type: 'stop' }] };
const d2: FlowDefinition = { ...d0, steps: [{ key: 'b', type: 'stop' }] };

const isValidate = (url: string) => url.endsWith('/validate');
const noIssues = () => new Response(JSON.stringify({ issues: [] }), { status: 200 });

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children);
const renderAutosave = (def: FlowDefinition) =>
  renderHook(({ def: d }) => useAutosave('flw_1', d, 1), { initialProps: { def }, wrapper });

/** A fetch stub whose draft saves wait until `release()` is called. */
function heldSaves() {
  const bodies: { definition: FlowDefinition; revision: number }[] = [];
  const waiting: (() => void)[] = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    if (isValidate(url)) return noIssues();
    bodies.push(JSON.parse(String(init?.body)) as { definition: FlowDefinition; revision: number });
    await new Promise<void>((resolve) => waiting.push(resolve));
    return new Response(JSON.stringify({ revision: bodies.length + 1, issues: [] }), { status: 200 });
  }));
  return { bodies, release: () => waiting.splice(0).forEach((r) => r()) };
}

describe('useAutosave', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('debounces saves, sends the revision and adopts the new one', async () => {
    const bodies: unknown[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
      if (isValidate(url)) return noIssues();
      bodies.push(JSON.parse(String(init?.body)));
      return new Response(JSON.stringify({ revision: bodies.length + 1, issues: [] }), { status: 200 });
    }));
    const { result, rerender } = renderAutosave(d0);
    expect(result.current.status).toBe('saved');
    rerender({ def: d1 });
    expect(result.current.status).toBe('pending');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1600);
    });
    expect(result.current.status).toBe('saved');
    expect(bodies).toEqual([{ definition: d1, revision: 1 }]);
    rerender({ def: d2 });
    await act(async () => {
      await result.current.flush();
    });
    expect(bodies[1]).toEqual({ definition: d2, revision: 2 });
  });

  it('returns to saved when edits are undone before saving', () => {
    vi.stubGlobal('fetch', vi.fn(async () => noIssues()));
    const { result, rerender } = renderAutosave(d0);
    rerender({ def: d1 });
    rerender({ def: d0 });
    expect(result.current.status).toBe('saved');
  });

  it('detects conflicts and stops saving until reset', async () => {
    const fetchMock = vi.fn(async (url: string) =>
      isValidate(url) ? noIssues() : new Response(JSON.stringify({ code: 'flow.revision_conflict', title: 'Conflict' }), { status: 409 }),
    );
    vi.stubGlobal('fetch', fetchMock);
    const { result, rerender } = renderAutosave(d0);
    rerender({ def: d1 });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1600);
    });
    expect(result.current.status).toBe('conflict');
    await act(async () => {
      expect(await result.current.flush()).toBe(false);
    });
    expect(fetchMock.mock.calls.filter(([url]) => !isValidate(url))).toHaveLength(1);
    act(() => result.current.reset(d1, 5));
    expect(result.current.status).toBe('saved');
  });

  it('asks the server for issues when a flow is opened, before any edit', async () => {
    const issue = { severity: 'error', path: '/trigger/input/project_id', code: 'field.required', message: '`project_id` is required' };
    const fetchMock = vi.fn(async (url: string) => {
      if (isValidate(url)) return new Response(JSON.stringify({ issues: [issue] }), { status: 200 });
      throw new Error(`unexpected ${url}`);
    });
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderAutosave(d0);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.issues).toEqual([issue]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('writes each saved draft and revision into the cached flow, so reopening starts from it', async () => {
    vi.stubGlobal('fetch', vi.fn(async (url: string) => (isValidate(url) ? noIssues() : new Response(JSON.stringify({ revision: 7, issues: [] }), { status: 200 }))));
    client.setQueryData<Partial<Flow>>(qk.flow('flw_1'), { id: 'flw_1', draft: d0, draft_revision: 1 });
    const { rerender } = renderAutosave(d0);
    rerender({ def: d1 });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1600);
    });
    expect(client.getQueryData(qk.flow('flw_1'))).toMatchObject({ draft: d1, draft_revision: 7 });
    expect(client.getQueryState(qk.flow('flw_1'))?.isInvalidated).toBe(true);
  });

  it('saves again when the definition was undone while a save was in flight', async () => {
    const { bodies, release } = heldSaves();
    const { result, rerender } = renderAutosave(d0);
    rerender({ def: d1 });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1600);
    });
    expect(bodies).toHaveLength(1);
    rerender({ def: d0 }); // Ctrl+Z while the request runs
    await act(async () => {
      release();
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.status).toBe('pending');
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1600);
      release();
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(bodies[1]).toEqual({ definition: d0, revision: 2 });
    expect(result.current.status).toBe('saved');
  });

  it('saves pending edits when the builder unmounts before the debounce fires', async () => {
    const { bodies, release } = heldSaves();
    const { rerender, unmount } = renderAutosave(d0);
    rerender({ def: d1 });
    unmount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(bodies).toEqual([{ definition: d1, revision: 1 }]);
    expect(client.isMutating()).toBe(1);
    await act(async () => {
      release();
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(client.isMutating()).toBe(0);
  });

  it('asks before the tab closes while changes are unsaved, and not once they are saved', async () => {
    vi.stubGlobal('fetch', vi.fn(async (url: string) => (isValidate(url) ? noIssues() : new Response(JSON.stringify({ revision: 2, issues: [] }), { status: 200 }))));
    const { rerender, result } = renderAutosave(d0);
    const leave = () => {
      const e = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(e);
      return e.defaultPrevented;
    };
    expect(leave()).toBe(false);
    rerender({ def: d1 });
    expect(leave()).toBe(true); // and starts saving right away
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(result.current.status).toBe('saved');
    expect(leave()).toBe(false);
  });
});
