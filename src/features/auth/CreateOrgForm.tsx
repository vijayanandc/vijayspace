// Creating an organisation: a name and a time zone (used by schedules and dates). The creator becomes its owner.

import { useMutation } from '@tanstack/react-query';
import { type FormEvent, useMemo, useState } from 'react';

import { errorMessage } from '../../api/client';
import { type Membership, tenancy } from '../../api/tenancy';
import { Button } from '../../components/Button';
import { Field } from '../../components/Field';
import { browserTimeZone, timeZones } from './timezones';

export function CreateOrgForm({ onCreated, onCancel }: { onCreated: (org: Membership) => void; onCancel?: () => void }) {
  const [name, setName] = useState('');
  const [timezone, setTimezone] = useState(browserTimeZone);
  const [touched, setTouched] = useState(false);
  const zones = useMemo(timeZones, []);
  const create = useMutation({ mutationFn: () => tenancy.createOrg({ name: name.trim(), timezone }), onSuccess: onCreated });
  const nameError = touched && name.trim() === '' ? 'Enter a name for the organisation.' : undefined;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (name.trim()) create.mutate();
  };
  return (
    <form className="create-org" onSubmit={submit} noValidate>
      <Field label="Organisation name" required error={nameError} help="Usually your company or team name. You can rename it later.">
        {(id) => <input id={id} className="input" value={name} maxLength={120} onChange={(e) => setName(e.target.value)} autoComplete="organization" />}
      </Field>
      <Field label="Time zone" help="Used for schedules and dates in flows.">
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
      {create.error && (
        <p className="field-error-text" role="alert">
          {errorMessage(create.error)}
        </p>
      )}
      <div className="form-actions">
        {onCancel && (
          <Button type="button" onClick={onCancel}>
            Cancel
          </Button>
        )}
        <Button type="submit" variant="primary" disabled={create.isPending}>
          {create.isPending ? 'Creating…' : 'Create organisation'}
        </Button>
      </div>
    </form>
  );
}
