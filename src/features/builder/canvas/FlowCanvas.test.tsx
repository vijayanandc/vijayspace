import { fireEvent, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Flow, FlowDefinition } from '../../../api/types';
import { renderWithProviders } from '../../../test/render';
import { BuilderContext, type BuilderCtx } from '../BuilderContext';
import { ROOT } from '../../../domain/tree';
import { FlowCanvas } from './FlowCanvas';

class NoResize {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const def: FlowDefinition = { schema_version: 1, trigger: { type: 'manual' }, steps: [{ key: 'wait', type: 'delay', seconds: 60 }] };

function renderCanvas() {
  const ctx: BuilderCtx = {
    flowId: 'flw_1', flow: { trigger: { kind: 'manual', token: 't', is_active: false } } as Flow, def, apply: vi.fn(), manifests: [],
    selected: null, select: vi.fn(), issues: new Map(), activePoint: null, setActivePoint: vi.fn(), drop: vi.fn(), insertItem: vi.fn(),
    deleteStep: vi.fn(), flush: async () => true, refetchFlow: vi.fn(),
  };
  const view = renderWithProviders(
    <BuilderContext.Provider value={ctx}>
      <FlowCanvas />
    </BuilderContext.Provider>,
  );
  const wrapper = (id: string) => {
    const el = view.container.querySelector<HTMLElement>(`.react-flow__node[data-id="${id}"]`);
    if (!el) throw new Error(`no node ${id}`);
    return el;
  };
  return { ctx, wrapper };
}

describe('FlowCanvas keyboard', () => {
  beforeEach(() => vi.stubGlobal('ResizeObserver', NoResize));
  afterEach(() => vi.unstubAllGlobals());

  it('opens the trigger and step panels with Enter or Space on a focused card', () => {
    const { ctx, wrapper } = renderCanvas();
    expect(wrapper('trigger')).toHaveAttribute('tabindex', '0');
    fireEvent.keyDown(wrapper('trigger'), { key: 'Enter' });
    expect(ctx.select).toHaveBeenLastCalledWith('trigger');
    fireEvent.keyDown(wrapper('step:wait'), { key: ' ' });
    expect(ctx.select).toHaveBeenLastCalledWith('wait');
  });

  it('activates an insert point with Enter or Space, which is its only tab stop', () => {
    const { ctx, wrapper } = renderCanvas();
    const plus = screen.getAllByLabelText('Insert a step here')[0];
    if (!plus) throw new Error('no insert point');
    fireEvent.keyDown(plus, { key: 'Enter' });
    expect(ctx.setActivePoint).toHaveBeenLastCalledWith({ list: ROOT, index: 0 });
    fireEvent.keyDown(plus, { key: ' ' });
    expect(ctx.setActivePoint).toHaveBeenCalledTimes(2);
    expect(wrapper('ins:root:0')).not.toHaveAttribute('tabindex');
  });
});
