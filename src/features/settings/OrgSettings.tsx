// Settings → Organisation: name and time zone (owners edit; everyone else sees them).

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Building2 } from 'lucide-react';
import { type FormEvent, useMemo, useState } from 'react';

import { errorMessage } from '../../api/client';
import { qk } from '../../api/hooks';
import { tenancy } from '../../api/tenancy';
import type { Org } from '../../api/types';
import { Button } from '../../components/Button';
import { Field } from '../../components/Field';
import { useToast } from '../../components/Toast';
import { timeZones } from '../auth/timezones';

export function OrgSettings({ org, editable }: { org: Org; editable: boolean }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [name, setName] = useState(org.name);
  const [timezone, setTimezone] = useState(org.timezone);
  const zones = useMemo(() => Array.from(new Set([org.timezone, ...timeZones()])), [org.timezone]);
  const save = useMutation({
    mutationFn: () => tenancy.updateOrg({ name: name.trim(), timezone }),
    onSuccess: (next) => {
      qc.setQueryData(qk.org, next);
      void qc.invalidateQueries({ queryKey: qk.me });
      toast.show('success', 'Organisation saved');
    },
  });
  const changed = name.trim() !== org.name || timezone !== org.timezone;
  const nameError = name.trim() === '' ? 'Enter a name.' : undefined;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!nameError && changed) save.mutate();
  };
  return (
    <section className="card settings-card" aria-labelledby="org-title">
      <header className="settings-card-head">
        <span className="settings-icon" style={{ background: '#0f6cbd' }} aria-hidden="true">
          <Building2 size={18} />
        </span>
        <div>
          <h2 id="org-title">Organisation</h2>
          <p className="muted small">{editable ? 'Name and time zone (used for schedules and dates).' : 'Only owners change these.'}</p>
        </div>
      </header>
      <form onSubmit={submit} noValidate>
        <fieldset disabled={!editable} className="plain-fieldset">
          <Field label="Name" required error={editable ? nameError : undefined}>
            {(id) => <input id={id} className="input" value={name} maxLength={120} onChange={(e) => setName(e.target.value)} />}
          </Field>
          <Field label="Time zone">
            {(id) => (
              <select id={id} className="input" value={timezone} onChange={(e) => setTimezone(e.target.value)}>
                {zones.map((z) => (
                  <option key={z} value={z}>
                    {z}
                  </option>
                ))}
              </select>
            )}
          </Field>
        </fieldset>
        <dl className="settings-facts">
          <div>
            <dt>Organisation ID</dt>
            <dd className="mono">{org.id}</dd>
          </div>
        </dl>
        {save.error && (
          <p className="field-error-text" role="alert">
            {errorMessage(save.error)}
          </p>
        )}
        {editable && (
          <div className="settings-actions">
            <Button type="submit" variant="primary" disabled={!changed || Boolean(nameError) || save.isPending}>
              {save.isPending ? 'Saving…' : 'Save organisation'}
            </Button>
          </div>
        )}
      </form>
    </section>
  );
}
