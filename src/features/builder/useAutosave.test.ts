import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { FlowDefinition } from '../../api/types';
import { useAutosave } from './useAutosave';

const d0: FlowDefinition = { schema_version: 1, trigger: { type: 'manual' }, steps: [] };
const d1: FlowDefinition = { ...d0, steps: [{ key: 'a', type: 'stop' }] };
const d2: FlowDefinition = { ...d0, steps: [{ key: 'b', type: 'stop' }] };

const isValidate = (url: string) => url.endsWith('/validate');
const noIssues = () => new Response(JSON.stringify({ issues: [] }), { status: 200 });

describe('useAutosave', () => {
  beforeEach(() => vi.useFakeTimers());
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
    const { result, rerender } = renderHook(({ def }) => useAutosave('flw_1', def, 1), { initialProps: { def: d0 } });
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
    const { result, rerender } = renderHook(({ def }) => useAutosave('flw_1', def, 1), { initialProps: { def: d0 } });
    rerender({ def: d1 });
    rerender({ def: d0 });
    expect(result.current.status).toBe('saved');
  });

  it('detects conflicts and stops saving until reset', async () => {
    const fetchMock = vi.fn(async (url: string) =>
      isValidate(url) ? noIssues() : new Response(JSON.stringify({ code: 'flow.revision_conflict', title: 'Conflict' }), { status: 409 }),
    );
    vi.stubGlobal('fetch', fetchMock);
    const { result, rerender } = renderHook(({ def }) => useAutosave('flw_1', def, 1), { initialProps: { def: d0 } });
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
    const { result } = renderHook(() => useAutosave('flw_1', d0, 1));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.issues).toEqual([issue]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
