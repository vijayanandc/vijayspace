// Edits a connection: rename it, change its settings or rotate its credentials (secrets are never shown; a settings
// change needs them again). OAuth connections are renamed here and signed in again with Reconnect.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Lock } from 'lucide-react';
import { useEffect, useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Connection, ConnectorManifest, JsonObject } from '../../api/types';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox } from '../../components/Empty';
import { Field } from '../../components/Field';
import { MessageBar } from '../../components/MessageBar';
import { useToast } from '../../components/Toast';
import { DynamicForm } from '../../forms/DynamicForm';
import { visibleFields } from '../../forms/visibility';
import { changedFields, editStart, secretsStillNeeded, settingChanged } from './model/edit';
import { saveAndTest } from './saveAndTest';

interface Props {
  connection: Connection | null;
  manifest: ConnectorManifest | undefined;
  onClose: () => void;
}

export function EditConnectionDialog({ connection, manifest, onClose }: Props) {
  const qc = useQueryClient();
  const toast = useToast();
  const [name, setName] = useState('');
  const [values, setValues] = useState<JsonObject>({});
  const [problem, setProblem] = useState<string | null>(null);
  const isOauth = manifest?.auth.kind === 'oauth2';
  const fields = isOauth ? [] : (manifest?.auth.fields ?? []);
  const config = connection?.config ?? {};
  useEffect(() => {
    // Opening (or closing) starts clean: typed secrets never outlive the dialog.
    setName(connection?.name ?? '');
    setValues(editStart(manifest?.auth.kind === 'oauth2' ? [] : (manifest?.auth.fields ?? []), connection?.config ?? {}));
    setProblem(null);
  }, [connection, manifest]);
  const shown = visibleFields(fields, values);
  const applies = (f: (typeof fields)[number]) => shown.includes(f);
  const missing = secretsStillNeeded(fields, config, values, applies);
  const dropsOptionalSecrets = settingChanged(fields, config, values, applies) && shown.some((f) => f.secret && f.required !== true);
  const save = useMutation({
    mutationFn: async () => {
      if (!connection) throw new Error('No connection');
      const changed = changedFields(shown, config, values);
      const body = { name: name.trim() || connection.name, ...(changed ? { fields: changed } : {}) };
      return saveAndTest(() => endpoints.updateConnection(connection.id, body), Boolean(changed));
    },
    onSuccess: (outcome) => {
      void qc.invalidateQueries({ queryKey: qk.connections });
      if (outcome.problem) {
        setProblem(outcome.problem);
        return;
      }
      toast.show('success', `Saved “${outcome.connection.name}”`);
      onClose();
    },
  });
  const { reset } = save;
  useEffect(() => reset(), [connection, reset]);
  return (
    <Dialog
      title="Edit connection"
      subtitle={manifest?.name}
      open={connection !== null}
      onClose={onClose}
      wide={fields.length > 0}
      footer={
        <>
          <Button onClick={onClose}>{problem ? 'Close' : 'Cancel'}</Button>
          <Button variant="primary" disabled={save.isPending || missing.length > 0} onClick={() => save.mutate()}>
            {fields.length > 0 ? 'Save and test' : 'Save'}
          </Button>
        </>
      }
    >
      <Field label="Connection name">{(id) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} />}</Field>
      {fields.length > 0 && manifest && (
        <>
          <DynamicForm fields={fields.map((f) => ({ ...f, mappable: false, required: f.secret ? false : f.required }))} values={values} onChange={setValues} env={{ connector: manifest.key, connectionId: null, requiresConnection: false, variables: [] }} />
          <p className="field-help with-icon">
            <Lock size={12} aria-hidden="true" /> Stored secrets are never shown. Leave them empty to keep them; to change another setting, enter them again.
          </p>
        </>
      )}
      {missing.length > 0 && <MessageBar intent="warning">Changing a setting needs the credentials again: enter {missing.join(', ')}.</MessageBar>}
      {dropsOptionalSecrets && <MessageBar intent="info">Secret headers not entered again are removed when a setting changes.</MessageBar>}
      {isOauth && <MessageBar intent="info">To change the account or the sign-in settings, use Reconnect (sign in again) in the row’s menu.</MessageBar>}
      {problem && !save.isPending && (
        <MessageBar intent="error" role="alert" title="Saved, but the connection test failed:">
          {problem.replace(/\.$/, '')}. Fix the details and save again.
        </MessageBar>
      )}
      {save.error && <ErrorBox message={errorMessage(save.error)} />}
    </Dialog>
  );
}
