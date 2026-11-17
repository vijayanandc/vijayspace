import { describe, expect, it } from 'vitest';

import type { FlowDefinition } from '../../../api/types';
import { TRIGGER_ID } from './paths';
import { renameStep } from './rename';
import { followSelection } from './selection';
import { deleteStep } from './tree';

const def: FlowDefinition = {
  schema_version: 1,
  trigger: { type: 'manual' },
  steps: [
    { key: 'wait', type: 'delay', seconds: 60 },
    { key: 'done', type: 'stop' },
  ],
};

describe('followSelection', () => {
  it('follows a renamed step back and forth (undo/redo of a key rename)', () => {
    const renamed = renameStep(def, 'wait', 'pause');
    expect(followSelection(renamed, def, 'pause')).toBe('wait');
    expect(followSelection(def, renamed, 'wait')).toBe('pause');
  });

  it('keeps selections that still exist and clears ones that are gone', () => {
    expect(followSelection(def, def, 'done')).toBe('done');
    expect(followSelection(def, deleteStep(def, 'done'), 'done')).toBeNull();
    expect(followSelection(def, deleteStep(def, 'done'), TRIGGER_ID)).toBe(TRIGGER_ID);
    expect(followSelection(def, def, null)).toBeNull();
  });
});
