// Select with static or dynamic (`options_from`) options; mappable selects can switch to a custom value.

import { useQuery } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { FieldSpec, JsonObject, JsonValue, OptionItem } from '../api/types';
import type { FormEnv } from './env';
import { MappingInput } from './MappingInput';

interface Props {
  id: string;
  field: FieldSpec;
  value: JsonValue | undefined;
  onChange: (v: JsonValue) => void;
  env: FormEnv;
  siblings: JsonObject;
}

export function useDynamicOptions(field: FieldSpec, env: FormEnv, siblings: JsonObject) {
  const deps = (field.depends_on ?? []).map((k) => siblings[k] ?? null);
  const ready = Boolean(field.options_from) && (!env.requiresConnection || Boolean(env.connectionId));
  return useQuery({
    queryKey: ['options', env.connector, field.options_from, env.connectionId ?? null, deps],
    queryFn: () => endpoints.options(env.connector, field.options_from ?? '', { connection_id: env.connectionId ?? null, input: siblings }),
    enabled: ready,
    staleTime: 60_000,
    retry: false,
  });
}

export function SelectInput({ id, field, value, onChange, env, siblings }: Props) {
  const dynamic = useDynamicOptions(field, env, siblings);
  const options: OptionItem[] = field.options_from ? (dynamic.data?.options ?? []) : (field.options ?? []);
  const current = typeof value === 'string' ? value : value === undefined || value === null ? '' : JSON.stringify(value);
  const isCustom = current !== '' && !options.some((o) => o.value === current) && (current.includes('{{') || !field.options_from || dynamic.isSuccess);
  const [custom, setCustom] = useState(isCustom);

  if (field.multiple) {
    const selected = Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
    return (
      <div className="multi-select" id={id} role="group" aria-label={field.label}>
        {options.map((o) => (
          <label key={o.value} className="check">
            <input
              type="checkbox"
              checked={selected.includes(o.value)}
              onChange={(e) => onChange(e.target.checked ? [...selected, o.value] : selected.filter((v) => v !== o.value))}
            />
            {o.label}
          </label>
        ))}
        {dynamic.isFetching && <span className="muted small">Loading options…</span>}
      </div>
    );
  }

  if (custom && field.mappable !== false) {
    return (
      <div>
        <MappingInput id={id} value={current} onChange={onChange} variables={env.variables} placeholder="Custom value or {{ variable }}" />
        <button type="button" className="link-btn" onClick={() => setCustom(false)}>
          Choose from list
        </button>
      </div>
    );
  }

  return (
    <div>
      <div className="select-row">
        <select id={id} className="input" value={current} onChange={(e) => onChange(e.target.value)}>
          <option value="">{dynamic.isFetching ? 'Loading…' : 'Select…'}</option>
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
          {current && !options.some((o) => o.value === current) && <option value={current}>{current}</option>}
        </select>
        {field.options_from && (
          <button type="button" className="icon-btn" title="Reload options" aria-label="Reload options" onClick={() => void dynamic.refetch()}>
            <RefreshCw size={14} className={dynamic.isFetching ? 'spin' : undefined} aria-hidden="true" />
          </button>
        )}
      </div>
      {field.options_from && env.requiresConnection && !env.connectionId && <div className="field-help">Choose a connection to load options.</div>}
      {dynamic.error && <div className="field-error-text">{errorMessage(dynamic.error)}</div>}
      {field.mappable !== false && (
        <button type="button" className="link-btn" onClick={() => setCustom(true)}>
          Enter custom value
        </button>
      )}
    </div>
  );
}
