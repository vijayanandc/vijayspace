import { describe, expect, it } from 'vitest';

import type { Condition } from '../../../api/types';
import { fromModel, isEditable, summarize, toModel } from './condition';

describe('condition model', () => {
  it('round-trips groups and sub-groups', () => {
    const cond: Condition = {
      any: [
        { all: [{ left: '{{ trigger.amount }}', op: 'gt', right: '100' }, { left: '{{ trigger.country }}', op: 'in', right: 'IN, US' }] },
        { left: '{{ trigger.vip }}', op: 'is_true' },
      ],
    };
    expect(isEditable(cond)).toBe(true);
    expect(fromModel(toModel(cond))).toEqual(cond);
  });

  it('wraps a bare comparison and drops right for unary operators', () => {
    const model = toModel({ left: 'a', op: 'is_empty', right: 'ignored' });
    expect(fromModel(model)).toEqual({ all: [{ left: 'a', op: 'is_empty' }] });
  });

  it('flags structures the builder cannot show', () => {
    expect(isEditable({ not: { left: 'a', op: 'eq', right: 'b' } })).toBe(false);
    expect(isEditable({ all: [{ any: [{ all: [] }] }] })).toBe(false);
  });

  it('keeps non-string operands readable and summarises', () => {
    const model = toModel({ all: [{ left: '{{ x }}', op: 'gt', right: 100, case_insensitive: true }] });
    expect(model.items[0]).toEqual({ left: '{{ x }}', op: 'gt', right: '100', case_insensitive: true });
    expect(summarize({ all: [{ left: '{{ trigger.amount }}', op: 'gt', right: 100 }] })).toBe('trigger.amount is greater than 100');
  });
});
