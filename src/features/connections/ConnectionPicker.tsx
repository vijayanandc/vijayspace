// Connection select for an app, with "+ New connection". An OAuth sign-in leaves the page: `beforeRedirect` saves
// first (e.g. the flow being edited) and `returnTo` is where the sign-in comes back to.

import { useState } from 'react';

import { useConnections } from '../../api/hooks';
import { Field } from '../../components/Field';
import { ConnectionDialog } from './ConnectionDialog';

interface Props {
  connector: string;
  value: string | null | undefined;
  onChange: (id: string | null) => void;
  /** The in-app page an OAuth sign-in returns to. */
  returnTo?: string;
  /** Runs before leaving for an OAuth sign-in; `false` cancels it. */
  beforeRedirect?: () => Promise<boolean>;
}

export function ConnectionPicker({ connector, value, onChange, returnTo, beforeRedirect }: Props) {
  const { data: connections = [], isLoading } = useConnections();
  const [creating, setCreating] = useState(false);
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
        returnTo={returnTo}
        beforeRedirect={beforeRedirect}
      />
    </>
  );
}
