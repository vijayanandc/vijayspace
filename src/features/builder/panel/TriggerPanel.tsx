import { useMutation } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../../api/client';
import { endpoints } from '../../../api/endpoints';
import type { AppTriggerNode, TriggerNode, TriggerType, WebhookTriggerNode } from '../../../api/types';
import { Button } from '../../../components/Button';
import { ErrorBox } from '../../../components/Empty';
import { Field } from '../../../components/Field';
import { JsonView } from '../../../components/JsonView';
import { useToast } from '../../../components/Toast';
import { TRIGGER_CHOICES } from '../../flows/NewFlowDialog';
import { useBuilder } from '../BuilderContext';
import { DynamicForm } from '../forms/DynamicForm';
import { defaultTrigger } from '../model/factories';
import { buildVariableTree } from '../model/variables';
import { ConnectionPicker } from './ConnectionPicker';
import { SchedulePanel } from './SchedulePanel';

function CopyField({ label, value }: { label: string; value: string }) {
  return (
    <Field label={label}>
      {(id) => (
        <div className="copy-row">
          <input id={id} className="input mono" readOnly value={value} onFocus={(e) => e.target.select()} />
          <Button onClick={() => void navigator.clipboard?.writeText(value)}>Copy</Button>
        </div>
      )}
    </Field>
  );
}

function SampleSection() {
  const { flowId, flow, refetchFlow } = useBuilder();
  const sample = flow.trigger?.sample;
  const sampleJson = JSON.stringify(sample ?? null);
  // Listening ends when a new sample arrives (compared by value) or after five minutes.
  const [listening, setListening] = useState<{ until: number; base: string } | null>(null);
  const listen = useMutation({ mutationFn: () => endpoints.listen(flowId), onSuccess: () => setListening({ until: Date.now() + 5 * 60_000, base: sampleJson }) });
  const refetchRef = useRef(refetchFlow);
  refetchRef.current = refetchFlow;
  useEffect(() => {
    if (!listening) return undefined;
    if (sampleJson !== listening.base) {
      setListening(null);
      return undefined;
    }
    const t = window.setInterval(() => (Date.now() > listening.until ? setListening(null) : refetchRef.current()), 2000);
    return () => window.clearInterval(t);
  }, [listening, sampleJson]);
  const until = listening?.until ?? null;
  return (
    <section className="panel-section">
      <h3>Sample data</h3>
      <p className="muted small">Send a real request to capture its fields for mapping in later steps.</p>
      <Button onClick={() => listen.mutate()} disabled={listen.isPending || until !== null}>
        {until !== null ? 'Listening… send a request now' : 'Listen for sample'}
      </Button>
      {listen.error && <ErrorBox message={errorMessage(listen.error)} />}
      {sample !== undefined && sample !== null && <JsonView label="Captured sample" value={sample} />}
    </section>
  );
}

function WebhookSection({ trigger, onChange }: { trigger: WebhookTriggerNode; onChange: (t: TriggerNode, tag?: string) => void }) {
  const { flow } = useBuilder();
  return (
    <>
      {flow.trigger?.webhook_url ? <CopyField label="Webhook URL" value={flow.trigger.webhook_url} /> : <p className="muted small">The URL appears after the first save.</p>}
      <label className="check">
        <input type="checkbox" checked={trigger.split_array === true} onChange={(e) => onChange({ ...trigger, split_array: e.target.checked })} />
        If the body is a JSON array, start one run per item
      </label>
      <details className="advanced">
        <summary>Advanced</summary>
        <Field label="Idempotency header" help="Requests repeating this header's value start only one run">
          {(id) => <input id={id} className="input" placeholder="Idempotency-Key" value={trigger.idempotency_header ?? ''} onChange={(e) => onChange({ ...trigger, idempotency_header: e.target.value || undefined }, 'webhook-adv')} />}
        </Field>
        <Field label="Secret header" help="Requests must send this header with the flow's secret">
          {(id) => <input id={id} className="input" placeholder="X-Weft-Key" value={trigger.secret_header ?? ''} onChange={(e) => onChange({ ...trigger, secret_header: e.target.value || undefined }, 'webhook-adv')} />}
        </Field>
        {trigger.secret_header && flow.trigger?.verify_token && <CopyField label="Secret value" value={flow.trigger.verify_token} />}
      </details>
      <SampleSection />
    </>
  );
}

function AppSection({ trigger, onChange }: { trigger: AppTriggerNode; onChange: (t: TriggerNode, tag?: string) => void }) {
  const { manifests, def, flow } = useBuilder();
  const withTriggers = manifests.filter((m) => m.triggers.length > 0);
  const manifest = manifests.find((m) => m.key === trigger.connector);
  const spec = manifest?.triggers.find((t) => t.key === trigger.trigger);
  const variables = buildVariableTree(def, undefined, manifests, flow.trigger?.sample);
  return (
    <>
      <Field label="App">
        {(id) => (
          <select id={id} className="input" value={trigger.connector} onChange={(e) => onChange({ ...trigger, connector: e.target.value, trigger: '', input: {}, connection_id: null })}>
            <option value="">Choose an app…</option>
            {withTriggers.map((m) => (
              <option key={m.key} value={m.key}>
                {m.icon} {m.name}
              </option>
            ))}
          </select>
        )}
      </Field>
      {manifest && (
        <Field label="Event" help={spec?.description}>
          {(id) => (
            <select id={id} className="input" value={trigger.trigger} onChange={(e) => onChange({ ...trigger, trigger: e.target.value, input: {} })}>
              <option value="">Choose an event…</option>
              {manifest.triggers.map((t) => (
                <option key={t.key} value={t.key}>
                  {t.name} ({t.kind})
                </option>
              ))}
            </select>
          )}
        </Field>
      )}
      {manifest && manifest.auth.kind !== 'none' && <ConnectionPicker connector={manifest.key} value={trigger.connection_id} onChange={(id) => onChange({ ...trigger, connection_id: id })} />}
      {spec && (
        <DynamicForm
          fields={spec.fields}
          values={trigger.input ?? {}}
          onChange={(input) => onChange({ ...trigger, input }, 'app-input')}
          env={{ connector: manifest?.key ?? '', connectionId: trigger.connection_id, requiresConnection: manifest?.auth.kind !== 'none', variables }}
        />
      )}
      {spec && (
        <details className="advanced">
          <summary>Advanced</summary>
          <Field label="Polling / reconciliation interval (seconds)" help="Polling triggers check this often; instant triggers re-check for missed events">
            {(id) => <input id={id} className="input" type="number" min={60} placeholder="300" value={trigger.poll_interval_secs ?? ''} onChange={(e) => onChange({ ...trigger, poll_interval_secs: e.target.value ? Number(e.target.value) : undefined }, 'app-adv')} />}
          </Field>
          <Field label="Debounce (seconds)" help="Collapse bursts of events for the same record into one run">
            {(id) => <input id={id} className="input" type="number" min={0} placeholder="0" value={trigger.debounce_secs ?? ''} onChange={(e) => onChange({ ...trigger, debounce_secs: e.target.value ? Number(e.target.value) : undefined }, 'app-adv')} />}
          </Field>
        </details>
      )}
      {spec?.kind === 'instant' && flow.trigger?.app_event_url && <CopyField label="Event URL" value={flow.trigger.app_event_url} />}
      {flow.trigger?.verify_token && spec?.setup_instructions && <CopyField label="Secret (X-Weft-Key)" value={flow.trigger.verify_token} />}
      {spec?.setup_instructions && (
        <section className="panel-section">
          <h3>Setup</h3>
          <pre className="instructions">{spec.setup_instructions}</pre>
        </section>
      )}
    </>
  );
}

function ManualSection() {
  const { flowId, flow } = useBuilder();
  const navigate = useNavigate();
  const toast = useToast();
  const run = useMutation({
    mutationFn: () => endpoints.runFlow(flowId, {}),
    onSuccess: (res) => navigate(`/history/${res.execution_id}`),
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  return (
    <section className="panel-section">
      <p className="muted small">Manual flows run with “Run now” (published version) or the Test button (draft).</p>
      <Button onClick={() => run.mutate()} disabled={!flow.published_version || run.isPending}>
        Run now
      </Button>
      {!flow.published_version && <p className="muted small">Publish the flow first.</p>}
    </section>
  );
}

export function TriggerPanel() {
  const { def, apply, issues } = useBuilder();
  const trigger = def.trigger;
  const onChange = (t: TriggerNode, tag?: string) => apply({ ...def, trigger: t }, tag);
  const changeType = (type: TriggerType) => {
    if (type === trigger.type) return;
    if (window.confirm('Change the trigger type? Its current settings will be lost.')) onChange(defaultTrigger(type));
  };
  const triggerIssues = issues.get('trigger') ?? [];
  return (
    <div className="panel-body">
      <Field label="Trigger type">
        {(id) => (
          <select id={id} className="input" value={trigger.type} onChange={(e) => changeType(e.target.value as TriggerType)}>
            {TRIGGER_CHOICES.map((c) => (
              <option key={c.type} value={c.type}>
                {c.label}
              </option>
            ))}
          </select>
        )}
      </Field>
      {triggerIssues.length > 0 && (
        <ul className="panel-issues">
          {triggerIssues.map((i) => (
            <li key={`${i.path}:${i.code}`} className={i.severity === 'error' ? 'issue-error' : 'issue-warning'}>
              {i.message}
            </li>
          ))}
        </ul>
      )}
      {trigger.type === 'webhook' && <WebhookSection trigger={trigger} onChange={onChange} />}
      {trigger.type === 'schedule' && <SchedulePanel spec={trigger.schedule} onChange={(schedule) => onChange({ ...trigger, schedule }, 'schedule')} />}
      {trigger.type === 'app' && <AppSection trigger={trigger} onChange={onChange} />}
      {trigger.type === 'manual' && <ManualSection />}
    </div>
  );
}
