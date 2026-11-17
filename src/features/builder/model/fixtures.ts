// Shared test fixtures for builder model tests.

import type { FlowDefinition, Step } from '../../../api/types';

export function action(key: string, input: Record<string, string> = {}): Step {
  return { key, type: 'action', connector: 'core.http', action: 'request', input };
}

export function sampleDefinition(): FlowDefinition {
  return {
    schema_version: 1,
    trigger: { type: 'webhook', split_array: false },
    steps: [
      action('first', { url: 'https://x.io' }),
      {
        key: 'dec',
        type: 'decision',
        branches: [
          { key: 'big', name: 'Big', condition: { all: [{ left: '{{ trigger.body.amount }}', op: 'gt', right: '100' }] }, steps: [action('notify', { text: '{{ steps.first.body.id }}' })] },
          { key: 'small', condition: { all: [] }, steps: [] },
        ],
        otherwise: [action('thank')],
      },
      action('last'),
    ],
  };
}
