import { describe, expect, it } from 'vitest';

import { duplicateRowIds, kvMatches, kvObject, kvPairs, kvRows } from './keyValue';

const ids = () => {
  let n = 0;
  return () => ++n;
};

describe('key/value rows', () => {
  it('reads objects and the list form', () => {
    expect(kvPairs({ a: '1', n: 2 })).toEqual([['a', '1'], ['n', '2']]);
    expect(kvPairs([{ key: 'x', value: 'y' }, { nope: 1 }])).toEqual([['x', 'y']]);
    expect(kvPairs(null)).toEqual([]);
  });

  it('keeps every row on key collisions: the first wins in the object, later ones are flagged', () => {
    const rows = [{ id: 1, key: 'a', value: '1' }, { id: 2, key: 'a', value: '2' }, { id: 3, key: '', value: 'x' }];
    expect(kvObject(rows)).toEqual({ a: '1' });
    expect([...duplicateRowIds(rows)]).toEqual([2]);
    expect(rows).toHaveLength(3);
  });

  it('keeps row order for numeric keys and recognises its own output', () => {
    const rows = kvRows({ name: 'x', other: '' }, ids());
    const typed = rows.map((r) => (r.key === 'other' ? { ...r, key: '10' } : r));
    expect(typed.map((r) => r.key)).toEqual(['name', '10']);
    expect(kvMatches(typed, kvObject(typed))).toBe(true);
    expect(kvMatches(typed, { name: 'x' })).toBe(false);
  });
});
