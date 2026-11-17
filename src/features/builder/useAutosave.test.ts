import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { FlowDefinition } from '../../api/types';
import { useAutosave } from './useAutosave';

const d0: FlowDefinition = { schema_version: 1, trigger: { type: 'manual' }, steps: [] };
const d1: FlowDefinition = { ...d0, steps: [{ key: 'a', type: 'stop' }] };
const d2: FlowDefinition = { ...d0, steps: [{ key: 'b', type: 'stop' }] };

describe('useAutosave', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('debounces saves, sends the revision and adopts the new one', async () => {
    const bodies: unknown[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: string, init?: RequestInit) => {
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
    vi.stubGlobal('fetch', vi.fn());
    const { result, rerender } = renderHook(({ def }) => useAutosave('flw_1', def, 1), { initialProps: { def: d0 } });
    rerender({ def: d1 });
    rerender({ def: d0 });
    expect(result.current.status).toBe('saved');
  });

  it('detects conflicts and stops saving until reset', async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ code: 'flow.revision_conflict', title: 'Conflict' }), { status: 409 }));
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
    expect(fetchMock).toHaveBeenCalledTimes(1);
    act(() => result.current.reset(d1, 5));
    expect(result.current.status).toBe('saved');
  });
});
