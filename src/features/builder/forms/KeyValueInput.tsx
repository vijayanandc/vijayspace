// Key → value rows (headers, query parameters, free-form objects).

import { X } from 'lucide-react';

import type { JsonObject, JsonValue } from '../../../api/types';
import type { VarNode } from '../model/variables';
import { MappingInput } from './MappingInput';

function toPairs(value: JsonValue | undefined): [string, string][] {
  if (Array.isArray(value)) {
    return value.flatMap((item) =>
      item && typeof item === 'object' && !Array.isArray(item) && typeof item.key === 'string'
        ? [[item.key, typeof item.value === 'string' ? item.value : JSON.stringify(item.value ?? '')] as [string, string]]
        : [],
    );
  }
  if (value && typeof value === 'object') {
    return Object.entries(value).map(([k, v]) => [k, typeof v === 'string' ? v : JSON.stringify(v)]);
  }
  return [];
}

/** Rows of key → (mappable) value, stored as an object. Rows with empty keys are kept while editing. */
export function KeyValueInput({ value, onChange, variables, label }: { value: JsonValue | undefined; onChange: (v: JsonObject) => void; variables: VarNode[]; label: string }) {
  const pairs = toPairs(value);
  const emit = (next: [string, string][]) => {
    const obj: JsonObject = {};
    next.forEach(([k, v]) => {
      obj[k] = v;
    });
    onChange(obj);
  };
  return (
    <div className="kv">
      {pairs.map(([k, v], i) => (
        <div className="kv-row" key={i}>
          <input className="input kv-key" aria-label={`${label} key ${i + 1}`} value={k} onChange={(e) => emit(pairs.map((p, j) => (j === i ? [e.target.value, p[1]] : p)))} />
          <MappingInput ariaLabel={`${label} value ${i + 1}`} value={v} variables={variables} onChange={(nv) => emit(pairs.map((p, j) => (j === i ? [p[0], nv] : p)))} />
          <button type="button" className="icon-btn icon-btn-sm" aria-label={`Remove ${label} row ${i + 1}`} onClick={() => emit(pairs.filter((_, j) => j !== i))}>
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      ))}
      <button type="button" className="link-btn" onClick={() => emit([...pairs, [pairs.some(([k]) => k === '') ? `key_${pairs.length + 1}` : '', '']])}>
        + Add row
      </button>
    </div>
  );
}
