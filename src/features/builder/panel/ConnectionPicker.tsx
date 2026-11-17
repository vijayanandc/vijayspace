// Connection select for an app, with "+ New connection" (from the builder, OAuth saves the flow first and returns to it).

import { useState } from 'react';

import { useConnections } from '../../../api/hooks';
import { Field } from '../../../components/Field';
import { ConnectionDialog } from '../../connections/ConnectionDialog';
import { useOptionalBuilder } from '../BuilderContext';

interface Props {
  connector: string;
  value: string | null | undefined;
  onChange: (id: string | null) => void;
}

export function ConnectionPicker({ connector, value, onChange }: Props) {
  const { data: connections = [], isLoading } = useConnections();
  const [creating, setCreating] = useState(false);
  const builder = useOptionalBuilder();
  const mine = connections.filter((c) => c.connector === connector);
  const current = mine.find((c) => c.id === value);
  return (
    <>
      <Field
        label="Connection"
        required
        actions={
          <button type="button" className="link-btn" onClick={() => setCreating(true)}>
            + New connection
          </button>
        }
        help={current && current.status !== 'active' ? `This connection is ${current.status.replace('_', ' ')}.` : undefined}
      >
        {(id) => (
          <select id={id} className="input" value={value ?? ''} onChange={(e) => onChange(e.target.value || null)}>
            <option value="">{isLoading ? 'Loading…' : mine.length ? 'Choose a connection…' : 'No connections yet'}</option>
            {mine.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
                {c.status !== 'active' ? ` (${c.status})` : ''}
              </option>
            ))}
            {value && !current && <option value={value}>{value}</option>}
          </select>
        )}
      </Field>
      <ConnectionDialog
        open={creating}
        onClose={() => setCreating(false)}
        connector={connector}
        onCreated={(c) => onChange(c.id)}
        returnTo={builder ? `/flows/${builder.flowId}` : undefined}
        beforeRedirect={builder?.flush}
      />
    </>
  );
}
