// Condition builder model: one top-level AND/OR group of rows and one level of sub-groups.

import type { Comparison, Condition, JsonValue, Op } from '../../../api/types';

export type Mode = 'all' | 'any';

export interface Row {
  left: string;
  op: Op;
  right: string;
  case_insensitive: boolean;
}

export interface SubGroup {
  mode: Mode;
  rows: Row[];
}

export interface Group {
  mode: Mode;
  items: (Row | SubGroup)[];
}

export const NO_RIGHT: readonly Op[] = ['is_empty', 'is_not_empty', 'is_true', 'is_false', 'exists', 'not_exists'];

export const OP_LABELS: Record<Op, string> = {
  eq: 'equals', neq: 'does not equal', gt: 'is greater than', gte: 'is at least', lt: 'is less than', lte: 'is at most',
  contains: 'contains', not_contains: 'does not contain', starts_with: 'starts with', ends_with: 'ends with',
  in: 'is one of', not_in: 'is not one of', is_empty: 'is empty', is_not_empty: 'is not empty', is_true: 'is true',
  is_false: 'is false', matches_regex: 'matches regex', date_before: 'is before (date)', date_after: 'is after (date)',
  exists: 'exists', not_exists: 'does not exist',
};

export function isSubGroup(item: Row | SubGroup): item is SubGroup {
  return 'rows' in item;
}

export function emptyRow(): Row {
  return { left: '', op: 'eq', right: '', case_insensitive: false };
}

function text(value: JsonValue | undefined): string {
  if (value === undefined || value === null) return '';
  if (typeof value === 'string') return value;
  if (Array.isArray(value) && value.every((v) => typeof v === 'string' || typeof v === 'number')) return value.join(', ');
  return JSON.stringify(value);
}

function isComparison(c: Condition): c is Comparison {
  return 'op' in c;
}

function toRow(c: Comparison): Row {
  return { left: text(c.left), op: c.op, right: text(c.right), case_insensitive: c.case_insensitive === true };
}

/** True when the condition fits the builder (otherwise the panel shows a JSON editor). */
export function isEditable(c: Condition): boolean {
  if (isComparison(c)) return true;
  if ('not' in c) return false;
  const children = 'all' in c ? c.all : c.any;
  return children.every((child) => {
    if (isComparison(child)) return true;
    if ('not' in child) return false;
    return ('all' in child ? child.all : child.any).every(isComparison);
  });
}

export function toModel(c: Condition): Group {
  if (isComparison(c)) return { mode: 'all', items: [toRow(c)] };
  if ('not' in c) return { mode: 'all', items: [] };
  const mode: Mode = 'all' in c ? 'all' : 'any';
  const children = 'all' in c ? c.all : c.any;
  const items: (Row | SubGroup)[] = [];
  for (const child of children) {
    if (isComparison(child)) items.push(toRow(child));
    else if ('all' in child) items.push({ mode: 'all', rows: child.all.filter(isComparison).map(toRow) });
    else if ('any' in child) items.push({ mode: 'any', rows: child.any.filter(isComparison).map(toRow) });
  }
  return { mode, items };
}

function fromRow(r: Row): Comparison {
  const c: Comparison = { left: r.left, op: r.op };
  if (!NO_RIGHT.includes(r.op)) c.right = r.right;
  if (r.case_insensitive) c.case_insensitive = true;
  return c;
}

export function fromModel(g: Group): Condition {
  const children: Condition[] = g.items.map((item) =>
    isSubGroup(item)
      ? item.mode === 'all'
        ? { all: item.rows.map(fromRow) }
        : { any: item.rows.map(fromRow) }
      : fromRow(item),
  );
  return g.mode === 'all' ? { all: children } : { any: children };
}

/** One-line summary for edge labels: `trigger.body.amount > 100`. */
export function summarize(c: Condition): string {
  const g = toModel(c);
  const rows = g.items.flatMap((i) => (isSubGroup(i) ? i.rows : [i]));
  if (rows.length === 0) return 'condition';
  const first = rows[0];
  if (!first) return 'condition';
  const left = first.left.replace(/^\{\{\s*|\s*\}\}$/g, '');
  const one = `${left || '…'} ${OP_LABELS[first.op]}${NO_RIGHT.includes(first.op) ? '' : ` ${first.right}`}`;
  return rows.length > 1 ? `${one} ${g.mode === 'all' ? 'and' : 'or'} …` : one;
}
