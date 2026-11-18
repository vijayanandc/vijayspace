import { describe, expect, it } from 'vitest';

import { action, sampleDefinition } from '../test/flowFixtures';
import { createStep } from './factories';
import { allKeys, slugKey, uniqueKey } from './keys';
import {
  addBranch, allSteps, deleteStep, findStep, insertStep, locate, moveBranch, moveStep, removeBranch, updateStep,
} from './tree';

const keys = (steps: { key: string }[]) => steps.map((s) => s.key);

describe('tree operations', () => {
  it('keeps the objects of everything an update does not change', () => {
    const def = sampleDefinition();
    const next = updateStep(def, 'first', (s) => ({ ...s, name: 'Fetch' }));
    expect(next.steps[0]).not.toBe(def.steps[0]);
    expect(next.steps[1]).toBe(def.steps[1]);
    const untouched = updateStep(def, 'missing', (s) => ({ ...s, name: 'x' }));
    expect(untouched.steps).toBe(def.steps);
    const inBranch = updateStep(def, 'notify', (s) => ({ ...s, name: 'Tell' }));
    const before = def.steps[1];
    const after = inBranch.steps[1];
    expect(after).not.toBe(before);
    if (before?.type === 'decision' && after?.type === 'decision') expect(after.otherwise).toBe(before.otherwise);
  });

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
