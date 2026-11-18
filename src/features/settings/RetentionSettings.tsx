// Settings → Run history: how long finished runs are kept (server default or 1–3650 days).

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { History } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Org } from '../../api/types';
import { Button } from '../../components/Button';
import { ErrorBox } from '../../components/Empty';
import { MessageBar } from '../../components/MessageBar';
import { useToast } from '../../components/Toast';
import { MAX_RETENTION_DAYS, MIN_RETENTION_DAYS, retentionBody, retentionChanged, retentionError, retentionForm, retentionNote, retentionShrink } from './retention';

export function RetentionSettings({ org, editable = true }: { org: Org; editable?: boolean }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [form, setForm] = useState(() => retentionForm(org));
  // The saved org can change (another tab, a save here): start again from it.
  const [seen, setSeen] = useState(org);
  if (seen !== org) {
    setSeen(org);
    setForm(retentionForm(org));
  }
  const [submitted, setSubmitted] = useState(false);
  const error = submitted || form.days.trim() !== '' ? retentionError(form) : null;
  const changed = retentionChanged(form, org);
  const save = useMutation({
    mutationFn: (body: { retention_days: number | null }) => endpoints.updateOrg(body),
    onSuccess: (next) => {
      qc.setQueryData(qk.org, next);
      setSubmitted(false);
      toast.show('success', `Saved. ${retentionNote(next.retention_days_effective)}.`);
    },
  });
  const onSave = () => {
    setSubmitted(true);
    const body = retentionBody(form);
    if (!body) return;
    const shrink = retentionShrink(form, org);
    if (
      shrink &&
      !window.confirm(
        `Keep runs for ${shrink.to} day${shrink.to === 1 ? '' : 's'} instead of ${shrink.from}? Finished runs older than that are deleted within 15 minutes. This cannot be undone.`,
      )
    )
      return;
    save.mutate(body);
  };
  const daysId = 'retention-days';
  const errorId = `${daysId}-error`;

  return (
    <section className="card settings-card" aria-labelledby="run-history-title">
      <header className="settings-card-head">
        <span className="settings-icon" style={{ background: '#ca5010' }} aria-hidden="true">
          <History size={18} />
        </span>
        <div>
          <h2 id="run-history-title">Run history</h2>
          <p className="muted small">How long Weft keeps finished runs (succeeded, failed or cancelled) with their step-by-step history.</p>
        </div>
      </header>
      <fieldset className="retention-choices" disabled={!editable}>
        <legend className="sr-only">Keep finished runs</legend>
        <label className={`retention-choice${form.useDefault ? ' choice-on' : ''}`}>
          <input type="radio" name="retention" checked={form.useDefault} onChange={() => setForm({ ...form, useDefault: true })} />
          <span>Use server default ({org.retention_days_default} days)</span>
        </label>
        <label className={`retention-choice${form.useDefault ? '' : ' choice-on'}`}>
          <input type="radio" name="retention" checked={!form.useDefault} onChange={() => setForm({ ...form, useDefault: false })} aria-label="Keep finished runs for a number of days" />
          <span className="retention-inline">
            Keep finished runs for
            <input
              id={daysId}
              className="input retention-days"
              inputMode="numeric"
              aria-label="Days to keep finished runs"
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? errorId : undefined}
              value={form.days}
              onChange={(e) => setForm({ useDefault: false, days: e.target.value })}
            />
            days
          </span>
        </label>
        {error && (
          <div id={errorId} className="field-error-text" role="alert">
            {error}
          </div>
        )}
        <div className="field-help">
          Between {MIN_RETENTION_DAYS} and {MAX_RETENTION_DAYS.toLocaleString('en-US')} days. Now: {retentionNote(org.retention_days_effective).toLowerCase()}
          {org.retention_days === null ? ' (server default)' : ''}.
        </div>
      </fieldset>
      <MessageBar intent="info">
        A cleanup runs every 15 minutes and deletes finished runs older than this, with their step history. Queued, running and waiting runs are never deleted.
      </MessageBar>
      {save.error && <ErrorBox message={errorMessage(save.error)} />}
      {!editable && <p className="muted small">Only owners change how long runs are kept.</p>}
      <div className="settings-actions" hidden={!editable}>
        <Button variant="primary" onClick={onSave} disabled={!changed || save.isPending}>
          {save.isPending ? 'Saving…' : 'Save'}
        </Button>
        <Button
          onClick={() => {
            setForm(retentionForm(org));
            setSubmitted(false);
          }}
          disabled={!changed || save.isPending}
        >
          Discard changes
        </Button>
      </div>
    </section>
  );
}
