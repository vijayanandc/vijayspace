import { describe, expect, it } from 'vitest';

import type { FieldSpec } from '../api/types';
import { dependenciesReady } from './dependencies';

const module: FieldSpec = { key: 'module', label: 'Module', type: 'select', required: true };
const account: FieldSpec = { key: 'account_id', label: 'Account', type: 'select' };
const values: FieldSpec = { key: 'values', label: 'Values', type: 'object', depends_on: ['module'] };
const project: FieldSpec = { key: 'project_id', label: 'Project', type: 'select', depends_on: ['account_id'] };

describe('dynamic field dependencies', () => {
  it('waits for required dependencies but not optional ones', () => {
    expect(dependenciesReady(values, {}, [module, values])).toBe(false);
    expect(dependenciesReady(values, { module: 'Leads' }, [module, values])).toBe(true);
    expect(dependenciesReady(project, {}, [account, project])).toBe(true);
  });

  it('never loads for a template, which only has a value when the flow runs', () => {
    expect(dependenciesReady(values, { module: '{{ trigger.body.module }}' }, [module, values])).toBe(false);
  });
});
