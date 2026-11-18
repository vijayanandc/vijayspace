// Key → value rows (headers, query parameters, free-form objects).

import { X } from 'lucide-react';
import { Fragment, useId, useRef, useState } from 'react';

import type { JsonObject, JsonValue } from '../api/types';
import { type KvRow, duplicateRowIds, kvMatches, kvObject, kvRows } from '../domain/keyValue';
import type { VarNode } from '../domain/variables';
import { MappingInput } from './MappingInput';

/** Rows of key → (mappable) value, stored as an object. The row list is local, so rows keep their order and survive key collisions. */
export function KeyValueInput({ value, onChange, variables, label }: { value: JsonValue | undefined; onChange: (v: JsonObject) => void; variables: VarNode[]; label: string }) {
  const uid = useId();
  const lastId = useRef(0);
  const nextId = () => ++lastId.current;
  const [rows, setRows] = useState<KvRow[]>(() => kvRows(value, nextId));
  const [seen, setSeen] = useState(value);
  if (value !== seen) {
    // An outside change (undo, another step): rebuild the rows unless they already store it.
    setSeen(value);
    if (!kvMatches(rows, value)) setRows(kvRows(value, nextId));
  }
  const update = (next: KvRow[]) => {
    const changed = JSON.stringify(kvObject(next)) !== JSON.stringify(kvObject(rows));
    setRows(next);
    if (changed) onChange(kvObject(next));
  };
  const edit = (id: number, patch: Partial<KvRow>) => update(rows.map((r) => (r.id === id ? { ...r, ...patch } : r)));
  const dupes = duplicateRowIds(rows);
  return (
    <div className="kv">
      {rows.map((row, i) => {
        const dupe = dupes.has(row.id);
        const warningId = `${uid}-dupe-${row.id}`;
        return (
          <Fragment key={row.id}>
            <div className="kv-row">
              <input
                className="input kv-key"
                aria-label={`${label} key ${i + 1}`}
                aria-invalid={dupe || undefined}
                aria-describedby={dupe ? warningId : undefined}
                value={row.key}
                onChange={(e) => edit(row.id, { key: e.target.value })}
              />
              <MappingInput ariaLabel={`${label} value ${i + 1}`} value={row.value} variables={variables} onChange={(v) => edit(row.id, { value: v })} />
              <button type="button" className="icon-btn icon-btn-sm" aria-label={`Remove ${label} row ${i + 1}`} onClick={() => update(rows.filter((r) => r.id !== row.id))}>
                <X size={14} aria-hidden="true" />
              </button>
            </div>
            {dupe && (
              <div id={warningId} className="field-error-text kv-warning">
                Another row already uses the key “{row.key}”; this row is not saved until you rename it.
              </div>
            )}
          </Fragment>
        );
      })}
      <button type="button" className="link-btn" onClick={() => update([...rows, { id: nextId(), key: '', value: '' }])}>
        + Add row
      </button>
    </div>
  );
}
