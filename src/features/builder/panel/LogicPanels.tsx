// "Configure" tabs of the delay, set-variable and stop steps.

import { Plus, X } from 'lucide-react';
import { useState } from 'react';

import type { DelayStep, JsonValue, SetVariableStep, StopStep } from '../../../api/types';
import { Field } from '../../../components/Field';
import { useBuilder } from '../BuilderContext';
import { IntInput } from '../../../forms/IntInput';
import { MappingInput } from '../../../forms/MappingInput';
import { ParsedMappingInput } from '../../../forms/ParsedMappingInput';
import { updateStep } from '../../../domain/tree';
import { DELAY_UNITS, MAX_DELAY_SECS, bestDelayUnit, changeDelayUnit, delaySeconds, delayUnit, parseLooseValue } from '../../../domain/values';
import { useVariables } from './useVariables';

const UNIT_LABELS: Record<number, string> = { 1: 'seconds', 60: 'minutes', 3600: 'hours', 86_400: 'days' };

export function DelayPanel({ step }: { step: DelayStep }) {
  const { def, apply } = useBuilder();
  const variables = useVariables(step.key);
  const mode = step.until !== undefined ? 'until' : 'for';
  const seconds = step.seconds ?? 3600;
  // The unit sticks once chosen, so typing 120 (minutes) is not re-shown as 2 hours mid-edit.
  const [chosenUnit, setChosenUnit] = useState(() => bestDelayUnit(seconds));
  const unit = delayUnit(seconds, chosenUnit);
  const units: readonly number[] = unit === 1 ? [1, ...DELAY_UNITS] : DELAY_UNITS;
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
              <IntInput id={id} min={1} max={Math.floor(MAX_DELAY_SECS / unit)} value={seconds / unit} onChange={(n) => n !== undefined && set({ ...base, seconds: delaySeconds(n, unit) }, `delay:${step.key}`)} />
            )}
          </Field>
          <Field label="Unit">
            {(id) => (
              <select
                id={id}
                className="input"
                value={unit}
                onChange={(e) => {
                  const next = Number(e.target.value);
                  setChosenUnit(next);
                  set({ ...base, seconds: changeDelayUnit(seconds, unit, next) });
                }}
              >
                {units.map((u) => (
                  <option key={u} value={u}>
                    {UNIT_LABELS[u]}
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
      <p className="field-help">Delays are durable: the run sleeps without holding a worker, up to 14 days.</p>
    </div>
  );
}

const assignmentText = (v: JsonValue | undefined) => (typeof v === 'string' ? v : JSON.stringify(v ?? ''));

export function SetVariablePanel({ step }: { step: SetVariableStep }) {
  const { def, apply } = useBuilder();
  const variables = useVariables(step.key);
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
          <ParsedMappingInput
            ariaLabel={`Variable ${i + 1} value`}
            value={a.value}
            parse={parseLooseValue}
            format={assignmentText}
            onChange={(value) => setAssignments(step.assignments.map((x, j) => (j === i ? { ...x, value } : x)), `var:${step.key}`)}
            variables={variables}
          />
          <button type="button" className="icon-btn icon-btn-sm" aria-label={`Remove variable ${i + 1}`} onClick={() => setAssignments(step.assignments.filter((_, j) => j !== i))}>
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      ))}
      <button type="button" className="btn btn-secondary btn-sm" onClick={() => setAssignments([...step.assignments, { name: `value_${step.assignments.length + 1}`, value: '' }])}>
        <Plus size={14} aria-hidden="true" />
        Add variable
      </button>
      <p className="field-help">Later steps read these as {'{{ vars.<name> }}'}. Values that are JSON (numbers, true/false, lists, objects) keep their type.</p>
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
