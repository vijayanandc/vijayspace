// Renders any connector's FieldSpec[] (actions, triggers, connection forms).

import { useQuery } from '@tanstack/react-query';

import { errorMessage } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { FieldSpec, JsonObject, JsonValue } from '../api/types';
import { Field } from '../components/Field';
import { fieldIssues } from '../domain/code';
import { parseNumberField } from '../domain/values';
import { CodeEditor } from './CodeEditor';
import type { FormEnv } from './env';
import { JsonInput } from './JsonInput';
import { visibleFields } from './visibility';
import { KeyValueInput } from './KeyValueInput';
import { MappingInput } from './MappingInput';
import { ParsedMappingInput } from './ParsedMappingInput';
import { SelectInput } from './SelectInput';
import { dependenciesReady, useDependencyKey } from './dependencies';

interface FieldProps {
  field: FieldSpec;
  value: JsonValue | undefined;
  onChange: (v: JsonValue) => void;
  env: FormEnv;
  siblings: JsonObject;
  /** The specs of the fields `siblings` holds (which dependencies are required). */
  siblingFields: FieldSpec[];
}

function asText(v: JsonValue | undefined): string {
  if (v === undefined || v === null) return '';
  return typeof v === 'string' ? v : JSON.stringify(v);
}

function ObjectFields({ field, value, onChange, env, siblings, siblingFields }: FieldProps) {
  const ready =
    Boolean(field.fields_from) && (!env.requiresConnection || Boolean(env.connectionId)) && dependenciesReady(field, siblings, siblingFields);
  const deps = useDependencyKey(field, siblings);
  const q = useQuery({
    queryKey: ['fields', env.connector, field.fields_from, env.connectionId ?? null, deps],
    queryFn: () => endpoints.fields(env.connector, field.fields_from ?? '', { connection_id: env.connectionId ?? null, input: siblings }),
    enabled: ready,
    staleTime: 60_000,
    gcTime: 60_000,
    retry: false,
  });
  const obj: JsonObject = value && typeof value === 'object' && !Array.isArray(value) ? value : {};
  if (!field.fields_from) return <KeyValueInput label={field.label} value={value} onChange={onChange} variables={env.variables} />;
  if (!ready) return <div className="field-help">Choose a connection{field.depends_on?.length ? ` and ${field.depends_on.join(', ')}` : ''} to load fields.</div>;
  if (q.isLoading) return <div className="muted small">Loading fields…</div>;
  if (q.error) return <div className="field-error-text">{errorMessage(q.error)}</div>;
  const fields = (q.data?.fields ?? []).filter((f) => !f.read_only);
  return (
    <div className="object-fields">
      <DynamicForm fields={fields} values={obj} onChange={onChange} env={env} />
    </div>
  );
}

export function FieldInput({ field, value, onChange, env, siblings, siblingFields }: FieldProps) {
  const mappable = field.mappable !== false;
  return (
    <Field label={field.label} required={field.required} help={field.help}>
      {(id) => {
        switch (field.type) {
          case 'select':
            return <SelectInput id={id} field={field} value={value} onChange={onChange} env={env} siblings={siblings} siblingFields={siblingFields} />;
          case 'boolean':
            if (typeof value === 'string' && value.includes('{{')) {
              return <MappingInput id={id} value={value} onChange={onChange} variables={env.variables} />;
            }
            return (
              <label className="check">
                <input id={id} type="checkbox" checked={value === true} onChange={(e) => onChange(e.target.checked)} />
                {field.help ? '' : 'Yes'}
              </label>
            );
          case 'json':
            return <JsonInput id={id} value={value} onChange={onChange} />;
          case 'key_value':
            return <KeyValueInput label={field.label} value={value} onChange={onChange} variables={env.variables} />;
          case 'object':
            return <ObjectFields field={field} value={value} onChange={onChange} env={env} siblings={siblings} siblingFields={siblingFields} />;
          case 'password':
            return <input id={id} className="input" type="password" autoComplete="new-password" value={asText(value)} onChange={(e) => onChange(e.target.value)} />;
          case 'number':
            if (!mappable) return <input id={id} className="input" type="number" value={asText(value)} onChange={(e) => onChange(e.target.value === '' ? null : Number(e.target.value))} />;
            return <ParsedMappingInput id={id} value={value} parse={parseNumberField} onChange={onChange} variables={env.variables} placeholder={field.placeholder} />;
          case 'code':
            return <CodeEditor id={id} value={asText(value)} onChange={onChange} issues={fieldIssues(env.issues, field.key)} placeholder={field.placeholder} />;
          case 'text':
            return mappable ? (
              <MappingInput id={id} multiline value={asText(value)} onChange={onChange} variables={env.variables} placeholder={field.placeholder} />
            ) : (
              <textarea id={id} className="input code" rows={6} value={asText(value)} onChange={(e) => onChange(e.target.value)} />
            );
          case 'string':
          case 'datetime':
            return mappable ? (
              <MappingInput id={id} value={asText(value)} onChange={onChange} variables={env.variables} placeholder={field.placeholder} />
            ) : (
              <input id={id} className="input" value={asText(value)} placeholder={field.placeholder} onChange={(e) => onChange(e.target.value)} />
            );
        }
      }}
    </Field>
  );
}

export function DynamicForm({ fields, values, onChange, env }: { fields: FieldSpec[]; values: JsonObject; onChange: (v: JsonObject) => void; env: FormEnv }) {
  const set = (key: string, v: JsonValue) => onChange({ ...values, [key]: v });
  // Fields another field hides (e.g. a password for another sign-in type) are not shown.
  const shown = visibleFields(fields, values);
  const basic = shown.filter((f) => !f.advanced);
  const advanced = shown.filter((f) => f.advanced);
  const render = (f: FieldSpec) => <FieldInput key={f.key} field={f} value={values[f.key]} onChange={(v) => set(f.key, v)} env={env} siblings={values} siblingFields={fields} />;
  return (
    <div className="dynamic-form">
      {basic.map(render)}
      {advanced.length > 0 && (
        <details className="advanced">
          <summary>Advanced</summary>
          {advanced.map(render)}
        </details>
      )}
      {fields.length === 0 && <p className="muted small">No fields to configure.</p>}
    </div>
  );
}
