import { describe, expect, it } from 'vitest';

import { action, sampleDefinition } from './fixtures';
import { createStep } from './factories';
import { historyReducer, initHistory } from './history';
import { allKeys, slugKey, uniqueKey } from './keys';
import { nodeForPointer } from './paths';
import { renameStep } from './rename';
import {
  addBranch, allSteps, deleteStep, findStep, insertStep, locate, moveBranch, moveStep, removeBranch, updateStep,
} from './tree';

const keys = (steps: { key: string }[]) => steps.map((s) => s.key);

describe('tree operations', () => {
  it('inserts into root and nested lists', () => {
    const def = sampleDefinition();
    const a = insertStep(def, { list: { kind: 'root' }, index: 1 }, action('x'));
    expect(keys(a.steps)).toEqual(['first', 'x', 'dec', 'last']);
    const b = insertStep(def, { list: { kind: 'branch', decision: 'dec', branch: 'small' }, index: 0 }, action('y'));
    expect(locate(b, 'y')?.ref).toEqual({ kind: 'branch', decision: 'dec', branch: 'small' });
    const c = insertStep(def, { list: { kind: 'otherwise', decision: 'dec' }, index: 99 }, action('z'));
    expect(locate(c, 'z')?.index).toBe(1);
    expect(def.steps).toHaveLength(3); // original untouched
  });

  it('deletes anywhere, including decisions with their children', () => {
    const def = sampleDefinition();
    expect(findStep(deleteStep(def, 'notify'), 'notify')).toBeUndefined();
    expect(allSteps(deleteStep(def, 'dec')).map((s) => s.key)).toEqual(['first', 'last']);
  });

  it('moves steps between lists and adjusts indexes within a list', () => {
    const def = sampleDefinition();
    expect(keys(moveStep(def, 'first', { list: { kind: 'root' }, index: 3 }).steps)).toEqual(['dec', 'last', 'first']);
    expect(moveStep(def, 'first', { list: { kind: 'root' }, index: 1 })).toBe(def); // no-op position
    const moved = moveStep(def, 'last', { list: { kind: 'branch', decision: 'dec', branch: 'big' }, index: 0 });
    expect(locate(moved, 'last')?.ref).toEqual({ kind: 'branch', decision: 'dec', branch: 'big' });
    expect(keys(moved.steps)).toEqual(['first', 'dec']);
  });

  it('refuses to move a decision into itself', () => {
    const def = sampleDefinition();
    expect(moveStep(def, 'dec', { list: { kind: 'otherwise', decision: 'dec' }, index: 0 })).toBe(def);
  });

  it('updates a nested step', () => {
    const def = updateStep(sampleDefinition(), 'notify', (s) => ({ ...s, name: 'Notify sales' }));
    expect(findStep(def, 'notify')?.name).toBe('Notify sales');
  });

  it('adds, moves and removes branches', () => {
    let def = addBranch(sampleDefinition(), 'dec');
    const dec = () => {
      const d = findStep(def, 'dec');
      if (d?.type !== 'decision') throw new Error('no decision');
      return d;
    };
    expect(dec().branches.map((b) => b.key)).toEqual(['big', 'small', 'branch_3']);
    def = moveBranch(def, 'dec', 'branch_3', -1);
    expect(dec().branches.map((b) => b.key)).toEqual(['big', 'branch_3', 'small']);
    def = removeBranch(def, 'dec', 'big');
    expect(dec().branches.map((b) => b.key)).toEqual(['branch_3', 'small']);
  });
});

describe('keys', () => {
  it('slugifies and makes keys unique', () => {
    expect(slugKey('Send Email!')).toBe('send_email');
    expect(slugKey('2nd step')).toBe('s_2nd_step');
    const def = insertStep(sampleDefinition(), { list: { kind: 'root' }, index: 0 }, action('send_email'));
    expect(uniqueKey(def, 'Send email')).toBe('send_email_2');
    expect(allKeys(def).has('notify')).toBe(true);
  });

  it('creates steps with unique keys and action defaults', () => {
    const def = sampleDefinition();
    const step = createStep(def, { kind: 'action', connector: 'core.http', action: 'first' }, []);
    expect(step.key).toBe('first_2');
    const decision = createStep(def, { kind: 'logic', type: 'decision' });
    expect(decision.type).toBe('decision');
  });
});

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
