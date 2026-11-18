// "Configure" tab of an action: app and action, connection, and the input form.

import type { ActionStep, JsonObject } from '../../../api/types';
import { Field } from '../../../components/Field';
import { useBuilder } from '../BuilderContext';
import { DynamicForm } from '../../../forms/DynamicForm';
import { updateStep } from '../../../domain/tree';
import { buildVariableTree } from '../../../domain/variables';
import { StepConnectionPicker } from './StepConnectionPicker';

export function ActionPanel({ step }: { step: ActionStep }) {
  const { def, apply, manifests, flow, issues } = useBuilder();
  const manifest = manifests.find((m) => m.key === step.connector);
  const spec = manifest?.actions.find((a) => a.key === step.action);
  const variables = buildVariableTree(def, step.key, manifests, flow.trigger?.sample);
  const set = (patch: Partial<ActionStep>, tag?: string) =>
    apply(updateStep(def, step.key, (s) => (s.type === 'action' ? { ...s, ...patch } : s)), tag);

  return (
    <div className="panel-body">
      <section className="panel-section">
        <h3 className="section-title">Action</h3>
        <Field label="App" required>
          {(id) => (
            <select id={id} className="input" value={step.connector} onChange={(e) => set({ connector: e.target.value, action: '', input: {}, connection_id: null })}>
              {!manifest && <option value={step.connector}>{step.connector || 'Choose…'}</option>}
              {manifests
                .filter((m) => m.actions.length > 0)
                .map((m) => (
                  <option key={m.key} value={m.key}>
                    {m.name}
                  </option>
                ))}
            </select>
          )}
        </Field>
        <Field label="Action" required help={spec?.description}>
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
      </section>
      {manifest && manifest.auth.kind !== 'none' && (
        <section className="panel-section">
          <h3 className="section-title">Connection</h3>
          <StepConnectionPicker connector={manifest.key} value={step.connection_id} onChange={(id) => set({ connection_id: id })} />
        </section>
      )}
      {spec && (
        <section className="panel-section">
          <h3 className="section-title">Input</h3>
          <DynamicForm
            fields={spec.fields}
            values={step.input ?? {}}
            onChange={(input: JsonObject) => set({ input }, `input:${step.key}`)}
            env={{ connector: step.connector, connectionId: step.connection_id, requiresConnection: manifest?.auth.kind !== 'none', variables, issues: issues.get(step.key) }}
          />
        </section>
      )}
    </div>
  );
}
