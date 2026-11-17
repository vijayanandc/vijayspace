// Creates a connection: a credentials form for `fields` auth, or the OAuth redirect for `oauth2`.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnectors } from '../../api/hooks';
import type { Connection, JsonObject } from '../../api/types';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox } from '../../components/Empty';
import { Field } from '../../components/Field';
import { DynamicForm } from '../builder/forms/DynamicForm';

interface Props {
  open: boolean;
  onClose: () => void;
  /** Preselects (and locks) the connector. */
  connector?: string;
  onCreated?: (c: Connection) => void;
}

export const OAUTH_RETURN_PATH = '/connections/callback';

export function ConnectionDialog({ open, onClose, connector, onCreated }: Props) {
  const { data: manifests = [] } = useConnectors();
  const choices = manifests.filter((m) => m.auth.kind !== 'none');
  const [key, setKey] = useState(connector ?? '');
  const [name, setName] = useState('');
  const [fields, setFields] = useState<JsonObject>({});
  const qc = useQueryClient();
  useEffect(() => {
    if (open) {
      setKey(connector ?? '');
      setFields({});
      setName('');
    }
  }, [open, connector]);
  const manifest = manifests.find((m) => m.key === key);
  const isOauth = manifest?.auth.kind === 'oauth2';
  const formFields = (manifest?.auth.fields ?? []).filter((f) => !(isOauth && f.secret));

  const create = useMutation({
    mutationFn: async () => {
      if (!manifest) throw new Error('Choose an app');
      const label = name.trim() || manifest.name;
      if (isOauth) {
        const res = await endpoints.oauthStart({ connector: manifest.key, name: label, fields, return_to: window.location.origin + OAUTH_RETURN_PATH });
        window.location.assign(res.authorize_url);
        return undefined;
      }
      return endpoints.createConnection({ connector: manifest.key, name: label, fields });
    },
    onSuccess: (conn) => {
      if (!conn) return;
      void qc.invalidateQueries({ queryKey: qk.connections });
      onCreated?.(conn);
      onClose();
    },
  });

  return (
    <Dialog
      title="New connection"
      open={open}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!manifest || create.isPending} onClick={() => create.mutate()}>
            {isOauth ? `Connect to ${manifest?.name ?? 'app'} →` : 'Save and test'}
          </Button>
        </>
      }
    >
      <Field label="App" required>
        {(id) => (
          <select id={id} className="input" value={key} disabled={Boolean(connector)} onChange={(e) => { setKey(e.target.value); setFields({}); }}>
            <option value="">Select an app…</option>
            {choices.map((m) => (
              <option key={m.key} value={m.key}>
                {m.icon} {m.name}
              </option>
            ))}
          </select>
        )}
      </Field>
      <Field label="Connection name" help="Shown when choosing a connection in a flow">
        {(id) => <input id={id} className="input" value={name} placeholder={manifest?.name ?? ''} onChange={(e) => setName(e.target.value)} />}
      </Field>
      {manifest && formFields.length > 0 && (
        <DynamicForm
          fields={formFields.map((f) => ({ ...f, mappable: false }))}
          values={fields}
          onChange={setFields}
          env={{ connector: manifest.key, connectionId: null, requiresConnection: false, variables: [] }}
        />
      )}
      {isOauth && <p className="muted small">You will be redirected to {manifest?.name} to sign in and approve access.</p>}
      {create.error && <ErrorBox message={errorMessage(create.error)} />}
    </Dialog>
  );
}
