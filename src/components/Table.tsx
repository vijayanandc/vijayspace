// Sortable column header for client-side tables.

import { ArrowDown, ArrowUp, ArrowUpDown } from 'lucide-react';

import type { SortState } from '../lib/sort';

interface Props<K extends string> {
  label: string;
  column: K;
  sort: SortState<K>;
  onSort: (column: K) => void;
  className?: string;
}

export function SortHeader<K extends string>({ label, column, sort, onSort, className }: Props<K>) {
  const active = sort.key === column;
  const Icon = !active ? ArrowUpDown : sort.dir === 'asc' ? ArrowUp : ArrowDown;
  return (
    <th className={className} aria-sort={active ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none'}>
      <button type="button" className={`th-sort${active ? ' th-sort-active' : ''}`} onClick={() => onSort(column)}>
        {label}
        <Icon size={14} aria-hidden="true" />
      </button>
    </th>
  );
}
