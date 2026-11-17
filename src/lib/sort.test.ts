import { describe, expect, it } from 'vitest';

import { nextSort, sortRows } from './sort';

describe('sort', () => {
  const rows = [
    { name: 'beta', n: 2 },
    { name: 'Alpha', n: null },
    { name: 'gamma', n: 10 },
  ];
  const value = (r: (typeof rows)[number], key: 'name' | 'n') => r[key];

  it('sorts strings case-insensitively and numbers numerically', () => {
    expect(sortRows(rows, { key: 'name', dir: 'asc' }, value).map((r) => r.name)).toEqual(['Alpha', 'beta', 'gamma']);
    expect(sortRows(rows, { key: 'n', dir: 'desc' }, value).map((r) => r.n)).toEqual([10, 2, null]);
  });

  it('keeps empty values last in both directions', () => {
    expect(sortRows(rows, { key: 'n', dir: 'asc' }, value).map((r) => r.n)).toEqual([2, 10, null]);
  });

  it('flips the direction on the active column and resets on a new one', () => {
    expect(nextSort({ key: 'name', dir: 'asc' }, 'name')).toEqual({ key: 'name', dir: 'desc' });
    expect(nextSort({ key: 'name', dir: 'desc' }, 'n')).toEqual({ key: 'n', dir: 'asc' });
  });
});
