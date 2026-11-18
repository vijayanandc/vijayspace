// Trigger "Settings" (advanced options per type) and "Test" (sample capture, Run now) tabs.

import { useMutation } from '@tanstack/react-query';
import { Play, Radio } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../../api/client';
import { endpoints } from '../../../api/endpoints';
import type { AppTriggerNode, TriggerNode, WebhookTriggerNode } from '../../../api/types';
import { Button } from '../../../components/Button';
import { ErrorBox } from '../../../components/Empty';
import { Field } from '../../../components/Field';
import { JsonView } from '../../../components/JsonView';
import { useToast } from '../../../components/Toast';
import { useBuilder } from '../BuilderContext';
import { IntInput } from '../../../forms/IntInput';
import { U32_MAX } from '../../../domain/values';
import { CopyField } from './CopyField';

type OnChange = (t: TriggerNode, tag?: string) => void;

function WebhookSettings({ trigger, onChange }: { trigger: WebhookTriggerNode; onChange: OnChange }) {
  const { flow } = useBuilder();
  return (
    <section className="panel-section">
      <h3 className="section-title">Request handling</h3>
      <Field label="Idempotency header" help="Requests repeating this header's value start only one run">
        {(id) => <input id={id} className="input" placeholder="Idempotency-Key" value={trigger.idempotency_header ?? ''} onChange={(e) => onChange({ ...trigger, idempotency_header: e.target.value || undefined }, 'webhook-adv')} />}
      </Field>
      <Field label="Secret header" help="Requests must send this header with the flow's secret">
        {(id) => <input id={id} className="input" placeholder="X-Weft-Key" value={trigger.secret_header ?? ''} onChange={(e) => onChange({ ...trigger, secret_header: e.target.value || undefined }, 'webhook-adv')} />}
      </Field>
      {trigger.secret_header && flow.trigger?.verify_token && <CopyField label="Secret value" value={flow.trigger.verify_token} />}
    </section>
  );
}

function AppSettings({ trigger, onChange }: { trigger: AppTriggerNode; onChange: OnChange }) {
  return (
    <section className="panel-section">
      <h3 className="section-title">Polling</h3>
      <Field label="Polling / reconciliation interval (seconds)" help="Polling triggers check this often; instant triggers re-check for missed events">
        {(id) => <IntInput id={id} className="input input-narrow" min={60} max={U32_MAX} optional placeholder="300" value={trigger.poll_interval_secs} onChange={(n) => onChange({ ...trigger, poll_interval_secs: n }, 'app-adv')} />}
      </Field>
      <Field label="Debounce (seconds)" help="Collapse bursts of events for the same record into one run">
        {(id) => <IntInput id={id} className="input input-narrow" min={0} max={U32_MAX} optional placeholder="0" value={trigger.debounce_secs} onChange={(n) => onChange({ ...trigger, debounce_secs: n }, 'app-adv')} />}
      </Field>
    </section>
  );
}

export function TriggerSettings() {
  const { def, apply, manifests } = useBuilder();
  const trigger = def.trigger;
  const onChange: OnChange = (t, tag) => apply({ ...def, trigger: t }, tag);
  if (trigger.type === 'webhook') return <div className="panel-body"><WebhookSettings trigger={trigger} onChange={onChange} /></div>;
  const spec = trigger.type === 'app' ? manifests.find((m) => m.key === trigger.connector)?.triggers.find((t) => t.key === trigger.trigger) : undefined;
  if (trigger.type === 'app' && spec) return <div className="panel-body"><AppSettings trigger={trigger} onChange={onChange} /></div>;
  return (
    <div className="panel-body">
      <p className="panel-empty">{trigger.type === 'app' ? 'Choose an app and event first.' : 'This trigger has no additional settings.'}</p>
    </div>
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
      <h3 className="section-title">Sample data</h3>
      <p className="field-help">Send a real request to capture its fields for mapping in later steps.</p>
      <Button icon={Radio} onClick={() => listen.mutate()} disabled={listen.isPending || until !== null}>
        {until !== null ? 'Listening… send a request now' : 'Listen for sample'}
      </Button>
      {listen.error && <ErrorBox message={errorMessage(listen.error)} />}
      {sample !== undefined && sample !== null && <JsonView label="Captured sample" value={sample} />}
    </section>
  );
}

function RunNow() {
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
      <h3 className="section-title">Run</h3>
      <p className="field-help">“Run now” starts the published version. Use Test in the top bar to run the draft.</p>
      <Button variant="primary" icon={Play} onClick={() => run.mutate()} disabled={!flow.published_version || run.isPending}>
        Run now
      </Button>
      {!flow.published_version && <p className="field-help">Publish the flow first.</p>}
    </section>
  );
}

export function TriggerTest() {
  const { def, flow } = useBuilder();
  const type = def.trigger.type;
  const sample = flow.trigger?.sample;
  return (
    <div className="panel-body">
      {type === 'webhook' && <SampleSection />}
      {type === 'manual' && <RunNow />}
      {(type === 'app' || type === 'schedule') && (
        <section className="panel-section">
          <h3 className="section-title">Sample data</h3>
          {sample !== undefined && sample !== null ? <JsonView label="Sample" value={sample} /> : <p className="field-help">No sample yet. Use Test in the top bar to run the draft with a payload.</p>}
        </section>
      )}
    </div>
  );
}
