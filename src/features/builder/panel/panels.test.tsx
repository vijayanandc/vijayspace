import { fireEvent, screen } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AppTriggerNode, ConnectorManifest, Flow, FlowDefinition, Step } from '../../../api/types';
import { renderWithProviders } from '../../../test/render';
import { type BuilderCtx, TestBuilderProvider } from '../BuilderContext';
import { allSteps } from '../../../domain/tree';
import { SidePanel } from './SidePanel';
import { TriggerSettings } from './TriggerExtras';

const manifests: ConnectorManifest[] = [
  {
    key: 'core.http', name: 'HTTP', description: '', icon: '', category: '', auth: { kind: 'none', fields: [] },
    actions: [{ key: 'request', name: 'Request', description: '', fields: [] }],
    triggers: [{ key: 'poll', name: 'Poll', description: '', kind: 'polling', fields: [] }],
  },
];

/** The side panel (or `children`) over a live definition; `latest()` reads what the panel stored. */
function renderPanel(initial: FlowDefinition, selectedKey: string | null, children?: JSX.Element) {
  let latest = initial;
  function Harness() {
    const [def, setDef] = useState(initial);
    const [selected, select] = useState(selectedKey);
    latest = def;
    const ctx = {
      flowId: 'flw_1', flow: { trigger: { kind: 'manual', token: 't', is_active: false } } as Flow, def, manifests, selected, select,
      apply: (d: FlowDefinition) => setDef(d), issues: new Map(), flush: async () => true, readOnly: false, deleteStep: vi.fn(),
    } as unknown as BuilderCtx;
    return (
      <TestBuilderProvider value={ctx}>
        {allSteps(def).map((s) => (
          <button key={s.key} type="button" onClick={() => select(s.key)}>
            pick {s.key}
          </button>
        ))}
        {children ?? <SidePanel />}
      </TestBuilderProvider>
    );
  }
  renderWithProviders(<Harness />);
  return { latest: () => latest, step: (key: string) => allSteps(latest).find((s) => s.key === key) };
}

const flowOf = (...steps: Step[]): FlowDefinition => ({ schema_version: 1, trigger: { type: 'manual' }, steps });

describe('side panel', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('does not carry one action’s test result into another action', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ input: {}, output: { from: 'a' } }), { status: 200 })));
    const action = (key: string): Step => ({ key, type: 'action', connector: 'core.http', action: 'request', input: {} });
    const { step } = renderPanel(flowOf(action('a'), action('b')), 'a');
    fireEvent.click(screen.getByRole('tab', { name: 'Test' }));
    fireEvent.click(screen.getByRole('button', { name: 'Test step' }));
    await screen.findByRole('button', { name: 'Use as sample' });

    fireEvent.click(screen.getByText('pick b'));
    fireEvent.click(screen.getByRole('tab', { name: 'Test' }));
    expect(screen.queryByRole('button', { name: 'Use as sample' })).not.toBeInTheDocument();
    expect(step('b')).not.toHaveProperty('sample_output');
  });

  it('starts each step’s panel fresh (the JSON condition editor does not carry over)', () => {
    const decision = (key: string): Step => ({
      key, type: 'decision', otherwise: [], branches: [{ key: 'branch_1', condition: { all: [{ left: key, op: 'eq', right: 'x' }] }, steps: [] }],
    });
    renderPanel(flowOf(decision('first'), decision('second')), 'first');
    fireEvent.click(screen.getByText('Edit as JSON'));
    expect((document.getElementById('condition-json') as HTMLTextAreaElement).value).toContain('"first"');
    fireEvent.click(screen.getByText('pick second'));
    expect(screen.getByLabelText('Condition 1 value')).toHaveValue('second');
    expect(screen.queryByText('Back to visual editor')).not.toBeInTheDocument();
  });

  it('keeps the delay a whole number of seconds while typing and switching units', () => {
    const { step } = renderPanel(flowOf({ key: 'wait', type: 'delay', seconds: 3600 }), 'wait');
    const amount = screen.getByLabelText('Amount');
    fireEvent.change(amount, { target: { value: '' } });
    expect(amount).toHaveValue(null);
    expect(step('wait')).toMatchObject({ seconds: 3600 });
    fireEvent.change(amount, { target: { value: '2.5' } });
    expect(step('wait')).toMatchObject({ seconds: 3 * 3600 });
    fireEvent.change(amount, { target: { value: '24' } });
    expect(screen.getByLabelText('Unit')).toHaveValue('3600'); // 24 hours stays in hours
    fireEvent.change(screen.getByLabelText('Unit'), { target: { value: '60' } });
    expect(step('wait')).toMatchObject({ seconds: 24 * 60 });
    fireEvent.change(screen.getByLabelText('Amount'), { target: { value: '120' } });
    expect(step('wait')).toMatchObject({ seconds: 7200 });
    expect(screen.getByLabelText('Amount')).toHaveValue(120);
  });

  it('stores max attempts as an integer between 1 and 20, or clears it', () => {
    const { step } = renderPanel(flowOf({ key: 'call', type: 'action', connector: 'core.http', action: 'request', input: {} }), 'call');
    fireEvent.click(screen.getByRole('tab', { name: 'Settings' }));
    const attempts = screen.getByLabelText(/Max attempts/);
    fireEvent.change(attempts, { target: { value: '3.7' } });
    expect(step('call')).toMatchObject({ retry: { max_attempts: 4 } });
    fireEvent.change(attempts, { target: { value: '-2' } });
    expect(step('call')).toMatchObject({ retry: { max_attempts: 1 } });
    fireEvent.change(attempts, { target: { value: '' } });
    expect(step('call')?.retry).toBeUndefined();
  });

  it('stores set-variable values that are JSON with their type', () => {
    const { step } = renderPanel(flowOf({ key: 'vars', type: 'set_variable', assignments: [{ name: 'ids', value: '' }] }), 'vars');
    const value = screen.getByLabelText('Variable 1 value');
    fireEvent.change(value, { target: { value: '[1, ' } });
    expect(value).toHaveValue('[1, ');
    fireEvent.change(value, { target: { value: '[1, 2]' } });
    expect(value).toHaveValue('[1, 2]');
    expect(step('vars')).toMatchObject({ assignments: [{ name: 'ids', value: [1, 2] }] });
    fireEvent.change(value, { target: { value: 'hello {{ trigger.name }}' } });
    expect(step('vars')).toMatchObject({ assignments: [{ name: 'ids', value: 'hello {{ trigger.name }}' }] });
  });

  it('stores the polling interval and debounce of an app trigger as integers', () => {
    const def: FlowDefinition = { schema_version: 1, trigger: { type: 'app', connector: 'core.http', trigger: 'poll', input: {} }, steps: [] };
    const { latest } = renderPanel(def, null, <TriggerSettings />);
    const poll = screen.getByLabelText(/Polling/);
    fireEvent.change(poll, { target: { value: '90.4' } });
    fireEvent.change(screen.getByLabelText(/Debounce/), { target: { value: '-1' } });
    expect(latest().trigger).toMatchObject({ poll_interval_secs: 90, debounce_secs: 0 });
    fireEvent.change(poll, { target: { value: '' } });
    expect((latest().trigger as AppTriggerNode).poll_interval_secs).toBeUndefined();
    expect(screen.getByLabelText(/Debounce/)).toHaveValue(-1); // the typed text stays until blur
  });
});
