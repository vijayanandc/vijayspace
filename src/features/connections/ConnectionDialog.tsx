// Creates a connection: a credentials form for `fields` auth, or the OAuth redirect for `oauth2`. A connection whose
// test fails is kept, and the dialog stays open with the app's reason so the details can be fixed and saved again.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { CircleCheck, ExternalLink, Lock } from 'lucide-react';
import { useEffect, useState } from 'react';

import { errorMessage, getCurrentOrg } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnectors } from '../../api/hooks';
import type { Connection, JsonObject } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox } from '../../components/Empty';
import { Field } from '../../components/Field';
import { MessageBar } from '../../components/MessageBar';
import { DynamicForm } from '../../forms/DynamicForm';
import { leaveForSignIn, oauthReturnUrl } from './oauth';
import { type SaveOutcome, saveAndTest } from './saveAndTest';

interface Props {
  open: boolean;
  onClose: () => void;
  /** Preselects (and locks) the connector. */
  connector?: string;
  onCreated?: (c: Connection) => void;
  /** In-app page to come back to after an OAuth sign-in (default: the callback page's own links). */
  returnTo?: string;
  /** Runs before leaving for the OAuth provider (e.g. saving the flow); false cancels the redirect. */
  beforeRedirect?: () => Promise<boolean>;
}

export function ConnectionDialog({ open, onClose, connector, onCreated, returnTo, beforeRedirect }: Props) {
  const { data: manifests = [] } = useConnectors();
  const choices = manifests.filter((m) => m.auth.kind !== 'none');
  const [key, setKey] = useState(connector ?? '');
  const [name, setName] = useState('');
  const [fields, setFields] = useState<JsonObject>({});
  // Saved, but its test failed: saving again updates it rather than creating a second one.
  const [saved, setSaved] = useState<SaveOutcome | null>(null);
  const qc = useQueryClient();
  const manifest = manifests.find((m) => m.key === key);
  const isOauth = manifest?.auth.kind === 'oauth2';
  // OAuth apps with their own client ("Any app (OAuth 2.0 sign-in)") take its secret here; the server keeps it encrypted
  // with the sign-in and never sends it to the browser again. Built-in OAuth apps have no secret fields.
  const formFields = manifest?.auth.fields ?? [];

  const create = useMutation({
    mutationFn: async (): Promise<SaveOutcome | undefined> => {
      if (!manifest) throw new Error('Choose an app');
      const label = name.trim() || manifest.name;
      if (isOauth) {
        if (beforeRedirect && !(await beforeRedirect())) throw new Error('Your latest flow changes could not be saved, so sign-in was not started. Fix the save error first.');
        const res = await endpoints.oauthStart({ connector: manifest.key, name: label, fields, return_to: oauthReturnUrl(window.location.origin, returnTo, getCurrentOrg()) });
        leaveForSignIn(res.authorize_url);
        return undefined;
      }
      const id = saved?.connection.id;
      return saveAndTest(() => (id ? endpoints.updateConnection(id, { name: label, fields }) : endpoints.createConnection({ connector: manifest.key, name: label, fields })), Boolean(id));
    },
    onSuccess: (outcome) => {
      if (!outcome) return;
      void qc.invalidateQueries({ queryKey: qk.connections });
      if (outcome.problem) {
        setSaved(outcome);
        return;
      }
      onCreated?.(outcome.connection);
      onClose();
    },
  });
  const { reset } = create;
  useEffect(() => {
    // Typed secrets do not outlive the dialog (closed, saved or cancelled), and reopening starts clean.
    setKey(connector ?? '');
    setFields({});
    setName('');
    setSaved(null);
    reset();
  }, [open, connector, reset]);
  const keepAnyway = () => {
    if (saved) onCreated?.(saved.connection);
    onClose();
  };

  return (
    <Dialog
      title="New connection"
      subtitle="Connect an app account once and reuse it in any flow."
      open={open}
      onClose={onClose}
      wide
      footer={
        <>
          {saved ? <Button onClick={keepAnyway}>Keep it and fix later</Button> : <Button onClick={onClose}>Cancel</Button>}
          <Button variant="primary" icon={isOauth ? ExternalLink : undefined} disabled={!manifest || create.isPending} onClick={() => create.mutate()}>
            {isOauth ? `Connect to ${manifest?.name ?? 'app'}` : 'Save and test'}
          </Button>
        </>
      }
    >
      <fieldset className="app-picker" disabled={Boolean(connector) || Boolean(saved)}>
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
      {saved?.problem && !create.isPending && (
        <MessageBar intent="error" role="alert" title="Saved, but the connection test failed:">
          {saved.problem.replace(/\.$/, '')}. Fix the details and choose Save and test again, or keep it and fix it later from Connections.
        </MessageBar>
      )}
      {create.error && <ErrorBox message={errorMessage(create.error)} />}
    </Dialog>
  );
}
