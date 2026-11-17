// Key → value rows (headers, query parameters): the editor keeps its own row list, so rows never
// collapse or reorder while typing, and the stored object is derived from it.

import type { JsonObject, JsonValue } from '../../../api/types';

export interface KvRow {
  id: number;
  key: string;
  value: string;
}

/** The pairs stored in `value`: an object, or the `[{key, value}]` list form. */
export function kvPairs(value: JsonValue | undefined): [string, string][] {
  const text = (v: JsonValue | undefined) => (typeof v === 'string' ? v : JSON.stringify(v ?? ''));
  if (Array.isArray(value)) {
    return value.flatMap((item) =>
      item && typeof item === 'object' && !Array.isArray(item) && typeof item.key === 'string' ? [[item.key, text(item.value)] as [string, string]] : [],
    );
  }
  if (value && typeof value === 'object') return Object.entries(value).map(([k, v]) => [k, text(v)]);
  return [];
}

export function kvRows(value: JsonValue | undefined, nextId: () => number): KvRow[] {
  return kvPairs(value).map(([key, v]) => ({ id: nextId(), key, value: v }));
}

/** The object to store: rows without a key are skipped, and the first row with a key wins. */
export function kvObject(rows: Pick<KvRow, 'key' | 'value'>[]): JsonObject {
  const out: JsonObject = {};
  for (const r of rows) {
    if (r.key !== '' && !Object.prototype.hasOwnProperty.call(out, r.key)) out[r.key] = r.value;
  }
  return out;
}

/** Ids of rows whose key an earlier row already uses (they are shown with a warning and not stored). */
export function duplicateRowIds(rows: KvRow[]): Set<number> {
  const seen = new Set<string>();
  const dupes = new Set<number>();
  for (const r of rows) {
    if (r.key === '') continue;
    if (seen.has(r.key)) dupes.add(r.id);
    seen.add(r.key);
  }
  return dupes;
}

function canonical(obj: JsonObject): string {
  return JSON.stringify(Object.entries(obj).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

/** True when the rows already store `value` (so an outside change does not need to rebuild them). */
export function kvMatches(rows: KvRow[], value: JsonValue | undefined): boolean {
  const pairs = kvPairs(value).map(([key, v]) => ({ key, value: v }));
  return canonical(kvObject(rows)) === canonical(kvObject(pairs));
}
