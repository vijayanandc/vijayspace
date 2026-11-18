// "Limits & usage" dialog: today's usage, and the connection's request limits (app default, no limit or a number).

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Info } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Connection, ConnectionLimits, LimitsView } from '../../api/types';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox, Loading } from '../../components/Empty';
import { MessageBar } from '../../components/MessageBar';
import { useToast } from '../../components/Toast';
import {
  LIMIT_KEYS, LIMIT_LABELS, type LimitField, type LimitKey, type LimitMode, type LimitsForm, formatLimit, limitError, limitsBody, limitsForm,
} from './limits';
import { UsageMeter } from './UsageMeter';
import { can } from '../auth/permissions';
import { useRole, useUserId } from '../auth/session';

interface Props {
  connection: Connection | null;
  appName: string;
  onClose: () => void;
}

const MODES: { mode: LimitMode; label: string }[] = [
  { mode: 'default', label: 'Use app default' },
  { mode: 'none', label: 'No limit' },
  { mode: 'custom', label: 'Custom' },
];

function LimitRow({ name, field, onChange, defaultValue, effective, showError, readOnly }: {
  name: LimitKey;
  field: LimitField;
  onChange: (f: LimitField) => void;
  defaultValue: number | null;
  effective: number | null;
  showError: boolean;
  /** Shown but not editable (the caller may not change this connection). */
  readOnly: boolean;
}) {
  const { label, unit, help } = LIMIT_LABELS[name];
  const error = showError || field.value.trim() !== '' ? limitError(field) : null;
  const inputId = `limit-${name}`;
  const hintId = `${inputId}-hint`;
  return (
    <fieldset className={`limit-row${error ? ' field-error' : ''}`} disabled={readOnly}>
      <legend className="field-label">{label}</legend>
      <div className="limit-controls">
        <div className="limit-modes" role="radiogroup" aria-label={`${label} setting`}>
          {MODES.map((m) => (
            <label key={m.mode} className={`limit-mode${field.mode === m.mode ? ' limit-mode-on' : ''}`}>
              <input type="radio" name={name} checked={field.mode === m.mode} onChange={() => onChange({ ...field, mode: m.mode })} />
              {m.mode === 'default' ? `${m.label} (${formatLimit(defaultValue)})` : m.label}
            </label>
          ))}
        </div>
        <span className="limit-input">
          <input
            id={inputId}
            className="input"
            inputMode="numeric"
            aria-label={`${label} (custom value)`}
            aria-describedby={hintId}
            aria-invalid={error ? true : undefined}
            placeholder={defaultValue ? String(defaultValue) : 'No default'}
            value={field.value}
            onChange={(e) => onChange({ mode: 'custom', value: e.target.value })}
          />
          <span className="limit-unit">{unit}</span>
        </span>
      </div>
      <div id={hintId} className="field-help">
        {help} App default: {formatLimit(defaultValue)}. In effect now: {formatLimit(effective)}.
      </div>
      {error && <div className="field-error-text">{error}</div>}
    </fieldset>
  );
}

export function LimitsDialog({ connection, appName, onClose }: Props) {
  const canManage = can(useRole(), 'connection.delete', connection?.created_by, useUserId());
  const qc = useQueryClient();
  const toast = useToast();
  const id = connection?.id ?? '';
  // Fresh on every open (gcTime 0), and not refetched while the form is being edited.
  const limits = useQuery({
    queryKey: qk.connectionLimits(id),
    queryFn: () => endpoints.connectionLimits(id),
    enabled: Boolean(connection),
    retry: false,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
  // The form starts from the loaded overrides, once per opening.
  const [state, setState] = useState<{ id: string; form: LimitsForm; submitted: boolean } | null>(null);
  if (!connection && state) setState(null);
  if (connection && limits.data && state?.id !== id) setState({ id, form: limitsForm(limits.data.overrides), submitted: false });
  const form = state?.id === id ? state.form : null;
  const body = form ? limitsBody(form) : null;
  const save = useMutation({
    mutationFn: (next: ConnectionLimits) => endpoints.setConnectionLimits(id, next),
    onSuccess: (next) => {
      qc.setQueryData(qk.connectionLimits(id), next);
      void qc.invalidateQueries({ queryKey: qk.connectionUsage });
      toast.show('success', `Saved the limits of “${connection?.name ?? id}”`);
      close();
    },
  });
  const close = () => {
    save.reset();
    onClose();
  };
  if (!connection) return null;
  const onSave = () => {
    setState((s) => (s ? { ...s, submitted: true } : s));
    if (body) save.mutate(body);
  };
  return (
    <Dialog
      title="Limits & usage"
      subtitle={`${connection.name} · ${appName}`}
      open
      onClose={close}
      wide
      footer={
        <>
          <Button onClick={close}>Cancel</Button>
          {canManage && (
            <Button variant="primary" onClick={onSave} disabled={!form || save.isPending}>
              {save.isPending ? 'Saving…' : 'Save limits'}
            </Button>
          )}
        </>
      }
    >
      {limits.error && <ErrorBox message={errorMessage(limits.error)} />}
      {!limits.error && (!limits.data || !form) && <Loading label="Loading limits…" />}
      {limits.data && form && state && (
        <LimitsEditor
          view={limits.data}
          form={form}
          submitted={state.submitted}
          readOnly={!canManage}
          onChange={(key, f) => setState((s) => (s ? { ...s, form: { ...s.form, [key]: f } } : s))}
        />
      )}
      {save.error && <ErrorBox message={errorMessage(save.error)} />}
    </Dialog>
  );
}

function LimitsEditor({ view, form, submitted, readOnly, onChange }: { view: LimitsView; form: LimitsForm; submitted: boolean; readOnly: boolean; onChange: (key: LimitKey, f: LimitField) => void }) {
  return (
    <>
      <section className="limits-usage" aria-label="Today’s usage">
        <div className="limits-usage-head">
          <strong>Today</strong>
          <span className="muted small">{view.usage.day} (UTC) · daily budget: {formatLimit(view.effective.daily_requests)}</span>
        </div>
        <UsageMeter usage={view.usage} detailed />
      </section>
      <MessageBar intent="info">
        Requests over a limit are retried later, never dropped. Background work (polling, backfill) pauses at 95% of the daily budget.
      </MessageBar>
      {LIMIT_KEYS.map((key) => (
        <LimitRow
          key={key}
          name={key}
          field={form[key]}
          onChange={(f) => onChange(key, f)}
          defaultValue={view.defaults[key]}
          effective={view.effective[key]}
          showError={submitted}
          readOnly={readOnly}
        />
      ))}
      <p className="field-help with-icon limits-note">
        <Info size={12} aria-hidden="true" /> Every request made through this connection counts: flow steps, triggers, polling and tests.
      </p>
    </>
  );
}
