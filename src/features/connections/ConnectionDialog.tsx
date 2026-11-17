// Creates a connection: a credentials form for `fields` auth, or the OAuth redirect for `oauth2`.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { CircleCheck, ExternalLink, Lock } from 'lucide-react';
import { useEffect, useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnectors } from '../../api/hooks';
import type { Connection, JsonObject } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox } from '../../components/Empty';
import { Field } from '../../components/Field';
import { MessageBar } from '../../components/MessageBar';
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
      subtitle="Connect an app account once and reuse it in any flow."
      open={open}
      onClose={onClose}
      wide
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" icon={isOauth ? ExternalLink : undefined} disabled={!manifest || create.isPending} onClick={() => create.mutate()}>
            {isOauth ? `Connect to ${manifest?.name ?? 'app'}` : 'Save and test'}
          </Button>
        </>
      }
    >
      <fieldset className="app-picker" disabled={Boolean(connector)}>
        <legend className="field-label">
          App<span className="required" aria-hidden="true"> *</span>
        </legend>
        {choices
          .filter((m) => !connector || m.key === connector)
          .map((m) => (
            <label key={m.key} className={`app-choice${key === m.key ? ' choice-active' : ''}`}>
              <input type="radio" name="connection-app" value={m.key} checked={key === m.key} onChange={() => { setKey(m.key); setFields({}); }} />
              <AppIcon app={m.key} size={32} decorative />
              <span className="choice-text">
                <strong>{m.name}</strong>
                <span className="choice-desc">{m.auth.kind === 'oauth2' ? 'Sign in with OAuth' : 'API credentials'}</span>
              </span>
              {key === m.key && <CircleCheck size={18} className="choice-check" aria-hidden="true" />}
            </label>
          ))}
      </fieldset>
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
      {isOauth && <MessageBar intent="info">You will be redirected to {manifest?.name} to sign in and approve access.</MessageBar>}
      {manifest && !isOauth && (
        <p className="field-help with-icon">
          <Lock size={12} aria-hidden="true" /> Credentials are encrypted on the server and never shown again.
        </p>
      )}
      {create.error && <ErrorBox message={errorMessage(create.error)} />}
    </Dialog>
  );
}
