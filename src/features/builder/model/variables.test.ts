import { describe, expect, it } from 'vitest';

import type { ConnectorManifest } from '../../../api/types';
import { sampleDefinition } from './fixtures';
import { buildVariableTree, childPath, jsonToVarNodes, stepsBefore } from './variables';

const http: ConnectorManifest = {
  key: 'core.http', name: 'HTTP', description: '', icon: '', category: '', auth: { kind: 'none', fields: [] },
  actions: [{ key: 'request', name: 'Request', description: '', fields: [], output_sample: { status: 200, body: { id: 'x' } } }],
  triggers: [],
};

describe('variables', () => {
  it('builds paths for identifiers, odd keys and arrays', () => {
    expect(childPath('trigger', 'body')).toBe('trigger.body');
    expect(childPath('trigger.body', 'first name')).toBe('trigger.body["first name"]');
    const nodes = jsonToVarNodes('steps.a', { items: [{ id: 1 }] });
    expect(nodes[0]?.children[0]?.path).toBe('steps.a.items[0]');
    expect(nodes[0]?.children[0]?.children[0]?.path).toBe('steps.a.items[0].id');
  });

  it('lists only steps before the current one, marking branch steps as maybe', () => {
    const def = sampleDefinition();
    expect(stepsBefore(def, 'first')).toEqual([]);
    expect(stepsBefore(def, 'notify').map((p) => p.step.key)).toEqual(['first']);
    const beforeLast = stepsBefore(def, 'last');
    expect(beforeLast.map((p) => `${p.step.key}:${p.maybe}`)).toEqual(['first:false', 'dec:false', 'notify:true', 'thank:true']);
  });

  it('uses captured samples and action output samples', () => {
    const def = sampleDefinition();
    const tree = buildVariableTree(def, 'notify', [http], { body: { amount: 250 } });
    expect(tree[0]?.children[0]?.children[0]?.path).toBe('trigger.body.amount');
    const steps = tree.find((n) => n.label === 'Steps');
    expect(steps?.children[0]?.children.map((c) => c.path)).toEqual(['steps.first.status', 'steps.first.body']);
  });
});
