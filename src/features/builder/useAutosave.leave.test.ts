// The save that runs when the builder unmounts: its failure can only be reported by a toast.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { type ReactNode, createElement } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { FlowDefinition } from '../../api/types';
import { useAutosave } from './useAutosave';

const show = vi.fn();
vi.mock('../../components/Toast', () => ({ useToast: () => ({ show }) }));

const d0: FlowDefinition = { schema_version: 1, trigger: { type: 'manual' }, steps: [] };
const d1: FlowDefinition = { ...d0, steps: [{ key: 'a', type: 'stop' }] };

describe('useAutosave on leave', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    show.mockReset();
  });

  it('tells the user when the save on leaving the builder fails', async () => {
    vi.stubGlobal('fetch', vi.fn(async (url: string) =>
      url.endsWith('/validate')
        ? new Response(JSON.stringify({ issues: [] }), { status: 200 })
        : new Response(JSON.stringify({ code: 'internal.error', title: 'Boom' }), { status: 500 }),
    ));
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children);
    const { rerender, unmount } = renderHook(({ def }) => useAutosave('flw_1', def, 1), { initialProps: { def: d0 }, wrapper });
    rerender({ def: d1 }); // an edit, then leaving before the debounce fires
    unmount();
    await act(async () => {
      await vi.waitFor(() => expect(show).toHaveBeenCalled());
    });
    expect(show).toHaveBeenCalledWith('error', expect.stringContaining('could not be saved'));
  });
});
