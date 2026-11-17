import { describe, expect, it } from 'vitest';

import type { FlowDefinition } from '../../api/types';
import { appChain, splitChain, triggerIconKey } from './appChain';

const action = (key: string, connector: string) => ({ key, type: 'action' as const, connector, action: 'x', input: {} });

describe('appChain', () => {
  it('starts with the trigger app and lists each action app once, depth-first', () => {
    const def: FlowDefinition = {
      schema_version: 1,
      trigger: { type: 'app', connector: 'zoho_crm', trigger: 'deal_won', input: {} },
      steps: [
        action('a', 'appblocks'),
        {
          key: 'd',
          type: 'decision',
          branches: [{ key: 'b', condition: { left: 1, op: 'eq', right: 1 }, steps: [action('c', 'core.email'), action('e', 'zoho_crm')] }],
          otherwise: [action('f', 'appblocks'), { key: 'w', type: 'delay', seconds: 60 }],
        },
      ],
    };
    expect(appChain(def)).toEqual(['zoho_crm', 'appblocks', 'core.email']);
  });

  it('uses the trigger type when there is no trigger app', () => {
    expect(triggerIconKey({ type: 'webhook' })).toBe('trigger:webhook');
    expect(triggerIconKey({ type: 'app', connector: '', trigger: '', input: {} })).toBe('trigger:app');
  });

  it('caps the chain and counts the rest', () => {
    expect(splitChain(['a', 'b', 'c'])).toEqual({ shown: ['a', 'b', 'c'], extra: 0 });
    expect(splitChain(['a', 'b', 'c', 'd', 'e', 'f'])).toEqual({ shown: ['a', 'b', 'c'], extra: 3 });
  });
});
