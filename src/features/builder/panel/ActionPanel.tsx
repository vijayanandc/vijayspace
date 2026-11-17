import type { ActionStep, JsonObject } from '../../../api/types';
import { Field } from '../../../components/Field';
import { useBuilder } from '../BuilderContext';
import { DynamicForm } from '../forms/DynamicForm';
import { updateStep } from '../model/tree';
import { buildVariableTree } from '../model/variables';
import { ConnectionPicker } from './ConnectionPicker';
import { TestStepBox } from './TestStepBox';

export function ActionPanel({ step }: { step: ActionStep }) {
  const { def, apply, manifests, flow } = useBuilder();
  const manifest = manifests.find((m) => m.key === step.connector);
  const spec = manifest?.actions.find((a) => a.key === step.action);
  const variables = buildVariableTree(def, step.key, manifests, flow.trigger?.sample);
  const set = (patch: Partial<ActionStep>, tag?: string) =>
    apply(updateStep(def, step.key, (s) => (s.type === 'action' ? { ...s, ...patch } : s)), tag);

  return (
    <div className="panel-body">
      <Field label="App">
        {(id) => (
          <select id={id} className="input" value={step.connector} onChange={(e) => set({ connector: e.target.value, action: '', input: {}, connection_id: null })}>
            {!manifest && <option value={step.connector}>{step.connector || 'Choose…'}</option>}
            {manifests
              .filter((m) => m.actions.length > 0)
              .map((m) => (
                <option key={m.key} value={m.key}>
                  {m.icon} {m.name}
                </option>
              ))}
          </select>
        )}
      </Field>
      <Field label="Action" help={spec?.description}>
        {(id) => (
          <select id={id} className="input" value={step.action} onChange={(e) => set({ action: e.target.value, input: {} })}>
            {!spec && <option value={step.action}>{step.action || 'Choose…'}</option>}
            {manifest?.actions.map((a) => (
              <option key={a.key} value={a.key}>
                {a.name}
              </option>
            ))}
          </select>
        )}
      </Field>
      {manifest && manifest.auth.kind !== 'none' && (
        <ConnectionPicker connector={manifest.key} value={step.connection_id} onChange={(id) => set({ connection_id: id })} />
      )}
      {spec && (
        <section className="panel-section">
          <h3>Input</h3>
          <DynamicForm
            fields={spec.fields}
            values={step.input ?? {}}
            onChange={(input: JsonObject) => set({ input }, `input:${step.key}`)}
            env={{ connector: step.connector, connectionId: step.connection_id, requiresConnection: manifest?.auth.kind !== 'none', variables }}
          />
        </section>
      )}
      <section className="panel-section">
        <h3>On failure</h3>
        <Field label="If this step fails permanently">
          {(id) => (
            <select id={id} className="input" value={step.on_error ?? 'fail_flow'} onChange={(e) => set({ on_error: e.target.value === 'continue' ? 'continue' : 'fail_flow' })}>
              <option value="fail_flow">Stop the flow (failed)</option>
              <option value="continue">Continue with the next step</option>
            </select>
          )}
        </Field>
        <Field label="Max attempts" help="Automatic retries for temporary errors (default 9: one try + 8 retries over ~16 h)">
          {(id) => (
            <input
              id={id}
              className="input"
              type="number"
              min={1}
              max={20}
              value={step.retry?.max_attempts ?? ''}
              placeholder="9"
              onChange={(e) => set({ retry: e.target.value ? { max_attempts: Math.max(1, Number(e.target.value)) } : undefined }, `retry:${step.key}`)}
            />
          )}
        </Field>
      </section>
      <TestStepBox stepKey={step.key} />
    </div>
  );
}
