// "Configure" tab of the trigger: its type and the main settings of each type.

import type { AppTriggerNode, TriggerNode, TriggerType, WebhookTriggerNode } from '../../../api/types';
import { Field } from '../../../components/Field';
import { formatTime } from '../../../lib/format';
import { TRIGGER_CHOICES } from '../../../domain/triggers';
import { useBuilder } from '../BuilderContext';
import { DynamicForm } from '../../../forms/DynamicForm';
import { defaultTrigger } from '../../../domain/factories';
import { useVariables } from './useVariables';
import { StepConnectionPicker } from './StepConnectionPicker';
import { CopyField } from './CopyField';
import { SchedulePanel } from './SchedulePanel';

function WebhookSection({ trigger, onChange }: { trigger: WebhookTriggerNode; onChange: (t: TriggerNode, tag?: string) => void }) {
  const { flow } = useBuilder();
  return (
    <section className="panel-section">
      <h3 className="section-title">Endpoint</h3>
      {flow.trigger?.webhook_url ? (
        <CopyField label="Webhook URL" value={flow.trigger.webhook_url} help="Send a POST request with a JSON body to start a run." />
      ) : (
        <p className="field-help">The URL appears after the first save.</p>
      )}
      <label className="check">
        <input type="checkbox" checked={trigger.split_array === true} onChange={(e) => onChange({ ...trigger, split_array: e.target.checked })} />
        If the body is a JSON array, start one run per item
      </label>
    </section>
  );
}

function AppSection({ trigger, onChange }: { trigger: AppTriggerNode; onChange: (t: TriggerNode, tag?: string) => void }) {
  const { manifests, flow } = useBuilder();
  const withTriggers = manifests.filter((m) => m.triggers.length > 0);
  const manifest = manifests.find((m) => m.key === trigger.connector);
  const spec = manifest?.triggers.find((t) => t.key === trigger.trigger);
  const variables = useVariables(undefined);
  return (
    <>
      <section className="panel-section">
        <h3 className="section-title">App event</h3>
        <Field label="App" required>
          {(id) => (
            <select id={id} className="input" value={trigger.connector} onChange={(e) => onChange({ ...trigger, connector: e.target.value, trigger: '', input: {}, connection_id: null })}>
              <option value="">Choose an app…</option>
              {withTriggers.map((m) => (
                <option key={m.key} value={m.key}>
                  {m.name}
                </option>
              ))}
            </select>
          )}
        </Field>
        {manifest && (
          <Field label="Event" required help={spec?.description}>
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
      </section>
      {manifest && manifest.auth.kind !== 'none' && (
        <section className="panel-section">
          <h3 className="section-title">Connection</h3>
          <StepConnectionPicker connector={manifest.key} value={trigger.connection_id} onChange={(id) => onChange({ ...trigger, connection_id: id })} />
        </section>
      )}
      {spec && (
        <section className="panel-section">
          <h3 className="section-title">Options</h3>
          <DynamicForm
            fields={spec.fields}
            values={trigger.input ?? {}}
            onChange={(input) => onChange({ ...trigger, input }, 'app-input')}
            env={{ connector: manifest?.key ?? '', connectionId: trigger.connection_id, requiresConnection: manifest?.auth.kind !== 'none', variables }}
          />
        </section>
      )}
      {spec && ((spec.kind === 'instant' && flow.trigger?.app_event_url) || spec.setup_instructions) && (
        <section className="panel-section">
          <h3 className="section-title">Setup</h3>
          {spec.kind === 'instant' && flow.trigger?.app_event_url && <CopyField label="Event URL" value={flow.trigger.app_event_url} />}
          {flow.trigger?.verify_token && spec.setup_instructions && <CopyField label="Secret (X-Weft-Key)" value={flow.trigger.verify_token} />}
          {spec.setup_instructions && <pre className="instructions">{spec.setup_instructions}</pre>}
        </section>
      )}
    </>
  );
}

export function TriggerPanel() {
  const { def, apply, flow } = useBuilder();
  const trigger = def.trigger;
  const onChange = (t: TriggerNode, tag?: string) => apply({ ...def, trigger: t }, tag);
  const changeType = (type: TriggerType) => {
    if (type === trigger.type) return;
    if (window.confirm('Change the trigger type? Its current settings will be lost.')) onChange(defaultTrigger(type));
  };
  return (
    <div className="panel-body">
      <section className="panel-section">
        <h3 className="section-title">Trigger</h3>
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
        {trigger.type === 'manual' && <p className="field-help">Manual flows run with “Run now” (published version) or the Test button (draft).</p>}
      </section>
      {trigger.type === 'webhook' && <WebhookSection trigger={trigger} onChange={onChange} />}
      {trigger.type === 'schedule' && (
        <section className="panel-section">
          <h3 className="section-title">Schedule</h3>
          <SchedulePanel spec={trigger.schedule} onChange={(schedule) => onChange({ ...trigger, schedule }, 'schedule')} />
          {flow.trigger?.next_fire_at && <p className="field-help">Next run: {formatTime(flow.trigger.next_fire_at)}</p>}
        </section>
      )}
      {trigger.type === 'app' && <AppSection trigger={trigger} onChange={onChange} />}
    </div>
  );
}
