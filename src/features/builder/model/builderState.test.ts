// Builder-only pure state: renaming a step (references follow), undo history, JSON pointers to canvas nodes.

import { describe, expect, it } from 'vitest';

import { deleteStep, findStep } from '../../../domain/tree';
import { sampleDefinition } from '../../../test/flowFixtures';
import { historyReducer, initHistory } from './history';
import { nodeForPointer } from './paths';
import { renameStep } from './rename';

describe('rename', () => {
  it('renames a step and rewrites references', () => {
    const def = renameStep(sampleDefinition(), 'first', 'fetch');
    expect(findStep(def, 'fetch')).toBeDefined();
    const notify = findStep(def, 'notify');
    expect(notify?.type === 'action' && notify.input.text).toBe('{{ steps.fetch.body.id }}');
  });

  it('refuses invalid or taken keys', () => {
    const def = sampleDefinition();
    expect(renameStep(def, 'first', 'last')).toBe(def);
    expect(renameStep(def, 'first', 'Bad Key')).toBe(def);
  });
});

describe('history', () => {
  it('undoes, redoes and coalesces tagged edits', () => {
    const d0 = sampleDefinition();
    const d1 = deleteStep(d0, 'first');
    const d2 = deleteStep(d1, 'last');
    let h = initHistory(d0);
    h = historyReducer(h, { type: 'apply', def: d1 });
    h = historyReducer(h, { type: 'apply', def: d2, tag: 'typing' });
    const d3 = { ...d2 };
    h = historyReducer(h, { type: 'apply', def: d3, tag: 'typing' });
    expect(h.past).toHaveLength(2);
    h = historyReducer(h, { type: 'undo' });
    expect(h.present).toBe(d1);
    h = historyReducer(h, { type: 'redo' });
    expect(h.present).toBe(d3);
    h = historyReducer(h, { type: 'undo' });
    h = historyReducer(h, { type: 'apply', def: d0 });
    expect(h.future).toHaveLength(0);
  });
});

describe('paths', () => {
  it('maps JSON pointers to nodes', () => {
    const def = sampleDefinition();
    expect(nodeForPointer(def, '/steps/0/input/url')).toBe('first');
    expect(nodeForPointer(def, '/steps/1/branches/0/steps/0/input/text')).toBe('notify');
    expect(nodeForPointer(def, '/steps/1/otherwise/0')).toBe('thank');
    expect(nodeForPointer(def, '/steps/1/branches/0/condition')).toBe('dec');
    expect(nodeForPointer(def, '/trigger/input')).toBe('trigger');
    expect(nodeForPointer(def, '/steps/9')).toBeUndefined();
  });
});
