// "Settings" tab of a step: the key (with safe rename), error handling for actions, and delete.

import { Trash2 } from 'lucide-react';
import { useState } from 'react';

import type { ActionStep, Step } from '../../../api/types';
import { Button } from '../../../components/Button';
import { Field } from '../../../components/Field';
import { useBuilder } from '../BuilderContext';
import { isValidKey } from '../model/keys';
import { renameStep } from '../model/rename';
import { allSteps, updateStep } from '../model/tree';

function ErrorHandling({ step }: { step: ActionStep }) {
  const { def, apply } = useBuilder();
  const set = (patch: Partial<ActionStep>, tag?: string) =>
    apply(updateStep(def, step.key, (s) => (s.type === 'action' ? { ...s, ...patch } : s)), tag);
  return (
    <section className="panel-section">
      <h3 className="section-title">Error handling</h3>
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
            className="input input-narrow"
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
  );
}

export function StepSettings({ step, onRenamed }: { step: Step; onRenamed: (key: string) => void }) {
  const { def, apply, select, deleteStep } = useBuilder();
  const [key, setKey] = useState(step.key);
  const [keyError, setKeyError] = useState<string | null>(null);
  const commitKey = () => {
    if (key === step.key) return;
    if (!isValidKey(key)) return setKeyError('Use lowercase letters, digits and _, starting with a letter');
    if (allSteps(def).some((s) => s.key === key)) return setKeyError('Another step already uses this key');
    setKeyError(null);
    apply(renameStep(def, step.key, key));
    onRenamed(key);
    select(key);
  };
  return (
    <div className="panel-body">
      <section className="panel-section">
        <h3 className="section-title">General</h3>
        <Field label="Step key" help="Used in variables as steps.<key>; references are updated on rename" error={keyError ?? undefined}>
          {(id) => <input id={id} className="input mono" value={key} onChange={(e) => setKey(e.target.value)} onBlur={commitKey} onKeyDown={(e) => e.key === 'Enter' && commitKey()} />}
        </Field>
      </section>
      {step.type === 'action' && <ErrorHandling step={step} />}
      <section className="panel-section">
        <h3 className="section-title">Delete</h3>
        <p className="field-help">Removes this step{step.type === 'decision' ? ' and every step inside its branches' : ''}. You can undo with Ctrl+Z.</p>
        <Button variant="danger" icon={Trash2} onClick={() => deleteStep(step.key)}>
          Delete step
        </Button>
      </section>
    </div>
  );
}
