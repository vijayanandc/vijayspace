import type { DelayStep, SetVariableStep, StopStep } from '../../../api/types';
import { Field } from '../../../components/Field';
import { useBuilder } from '../BuilderContext';
import { MappingInput } from '../forms/MappingInput';
import { updateStep } from '../model/tree';
import { buildVariableTree } from '../model/variables';

const UNITS = [
  { value: 60, label: 'minutes' },
  { value: 3600, label: 'hours' },
  { value: 86_400, label: 'days' },
];

function bestUnit(seconds: number): number {
  return [...UNITS].reverse().find((u) => seconds % u.value === 0)?.value ?? 60;
}

export function DelayPanel({ step }: { step: DelayStep }) {
  const { def, apply, manifests, flow } = useBuilder();
  const variables = buildVariableTree(def, step.key, manifests, flow.trigger?.sample);
  const mode = step.until !== undefined ? 'until' : 'for';
  const seconds = step.seconds ?? 3600;
  const unit = bestUnit(seconds);
  const set = (next: DelayStep, tag?: string) => apply(updateStep(def, step.key, () => next), tag);
  const base = { key: step.key, name: step.name, type: 'delay' as const };
  return (
    <div className="panel-body">
      <fieldset className="radio-row">
        <legend className="field-label">Wait</legend>
        <label className="check">
          <input type="radio" checked={mode === 'for'} onChange={() => set({ ...base, seconds: 3600 })} /> For a duration
        </label>
        <label className="check">
          <input type="radio" checked={mode === 'until'} onChange={() => set({ ...base, until: '' })} /> Until a date-time
        </label>
      </fieldset>
      {mode === 'for' ? (
        <div className="inline-fields">
          <Field label="Amount">
            {(id) => (
              <input id={id} className="input" type="number" min={1} value={seconds / unit} onChange={(e) => set({ ...base, seconds: Math.max(1, Math.round(Number(e.target.value) * unit)) }, `delay:${step.key}`)} />
            )}
          </Field>
          <Field label="Unit">
            {(id) => (
              <select id={id} className="input" value={unit} onChange={(e) => set({ ...base, seconds: (seconds / unit) * Number(e.target.value) })}>
                {UNITS.map((u) => (
                  <option key={u.value} value={u.value}>
                    {u.label}
                  </option>
                ))}
              </select>
            )}
          </Field>
        </div>
      ) : (
        <Field label="Until" help="A date-time (RFC 3339 or YYYY-MM-DD HH:MM, UTC) or a variable">
          {(id) => <MappingInput id={id} value={step.until ?? ''} onChange={(until) => set({ ...base, until }, `delay:${step.key}`)} variables={variables} placeholder="{{ trigger.body.due_at }}" />}
        </Field>
      )}
      <p className="muted small">Delays are durable: the run sleeps without holding a worker, up to 30 days.</p>
    </div>
  );
}

export function SetVariablePanel({ step }: { step: SetVariableStep }) {
  const { def, apply, manifests, flow } = useBuilder();
  const variables = buildVariableTree(def, step.key, manifests, flow.trigger?.sample);
  const setAssignments = (assignments: SetVariableStep['assignments'], tag?: string) =>
    apply(updateStep(def, step.key, (s) => (s.type === 'set_variable' ? { ...s, assignments } : s)), tag);
  return (
    <div className="panel-body">
      {step.assignments.map((a, i) => (
        <div key={i} className="assignment">
          <input
            className="input mono"
            aria-label={`Variable ${i + 1} name`}
            value={a.name}
            onChange={(e) => setAssignments(step.assignments.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)), `var:${step.key}`)}
          />
          <span aria-hidden="true">=</span>
          <MappingInput
            ariaLabel={`Variable ${i + 1} value`}
            value={typeof a.value === 'string' ? a.value : JSON.stringify(a.value)}
            onChange={(value) => setAssignments(step.assignments.map((x, j) => (j === i ? { ...x, value } : x)), `var:${step.key}`)}
            variables={variables}
          />
          <button type="button" className="icon-btn" aria-label={`Remove variable ${i + 1}`} onClick={() => setAssignments(step.assignments.filter((_, j) => j !== i))}>
            ×
          </button>
        </div>
      ))}
      <button type="button" className="link-btn" onClick={() => setAssignments([...step.assignments, { name: `value_${step.assignments.length + 1}`, value: '' }])}>
        + Add variable
      </button>
      <p className="muted small">Later steps read these as {'{{ vars.<name> }}'}.</p>
    </div>
  );
}

export function StopPanel({ step }: { step: StopStep }) {
  const { def, apply } = useBuilder();
  const set = (patch: Partial<StopStep>, tag?: string) => apply(updateStep(def, step.key, (s) => (s.type === 'stop' ? { ...s, ...patch } : s)), tag);
  return (
    <div className="panel-body">
      <Field label="End the flow as">
        {(id) => (
          <select id={id} className="input" value={step.status ?? 'succeeded'} onChange={(e) => set({ status: e.target.value === 'failed' ? 'failed' : 'succeeded' })}>
            <option value="succeeded">Succeeded</option>
            <option value="failed">Failed</option>
          </select>
        )}
      </Field>
      <Field label="Message">
        {(id) => <input id={id} className="input" value={step.message ?? ''} onChange={(e) => set({ message: e.target.value }, `stop:${step.key}`)} />}
      </Field>
    </div>
  );
}
