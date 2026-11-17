// Client-side sorting for list views (sortable column headers).

export type SortDir = 'asc' | 'desc';

export interface SortState<K extends string> {
  key: K;
  dir: SortDir;
}

export type SortValue = string | number | null | undefined;

/** Clicking the active column flips the direction; another column starts ascending. */
export function nextSort<K extends string>(current: SortState<K>, key: K): SortState<K> {
  if (current.key === key) return { key, dir: current.dir === 'asc' ? 'desc' : 'asc' };
  return { key, dir: 'asc' };
}

function compare(a: SortValue, b: SortValue): number {
  const aEmpty = a === null || a === undefined || a === '';
  const bEmpty = b === null || b === undefined || b === '';
  if (aEmpty || bEmpty) return aEmpty === bEmpty ? 0 : aEmpty ? 1 : -1;
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  return String(a).localeCompare(String(b), undefined, { sensitivity: 'base', numeric: true });
}

/** Returns a sorted copy. Empty values always sort last; ties keep their original order. */
export function sortRows<T, K extends string>(rows: T[], sort: SortState<K>, value: (row: T, key: K) => SortValue): T[] {
  const sign = sort.dir === 'asc' ? 1 : -1;
  return rows
    .map((row, index) => ({ row, index, v: value(row, sort.key) }))
    .sort((x, y) => {
      const xEmpty = x.v === null || x.v === undefined || x.v === '';
      const yEmpty = y.v === null || y.v === undefined || y.v === '';
      if (xEmpty !== yEmpty) return xEmpty ? 1 : -1;
      return sign * compare(x.v, y.v) || x.index - y.index;
    })
    .map((x) => x.row);
}
