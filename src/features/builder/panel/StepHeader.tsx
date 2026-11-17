import { useState } from 'react';

import type { Step } from '../../../api/types';
import { Button } from '../../../components/Button';
import { Field } from '../../../components/Field';
import { useBuilder } from '../BuilderContext';
import { isValidKey } from '../model/keys';
import { renameStep } from '../model/rename';
import { allSteps, updateStep } from '../model/tree';

/** Name, key (with safe rename) and delete for any step. */
export function StepHeader({ step }: { step: Step }) {
  const { def, apply, select, deleteStep, issues } = useBuilder();
  const [key, setKey] = useState(step.key);
  const [keyError, setKeyError] = useState<string | null>(null);
  const commitKey = () => {
    if (key === step.key) return;
    if (!isValidKey(key)) return setKeyError('Use lowercase letters, digits and _, starting with a letter');
    if (allSteps(def).some((s) => s.key === key)) return setKeyError('Another step already uses this key');
    setKeyError(null);
    apply(renameStep(def, step.key, key));
    select(key);
  };
  const stepIssues = issues.get(step.key) ?? [];
  return (
    <div className="panel-header">
      <Field label="Step name">
        {(id) => (
          <input
            id={id}
            className="input"
            value={step.name ?? ''}
            onChange={(e) => apply(updateStep(def, step.key, (s) => ({ ...s, name: e.target.value })), `name:${step.key}`)}
          />
        )}
      </Field>
      <Field label="Key" help="Used in variables as steps.<key>; references are updated on rename" error={keyError ?? undefined}>
        {(id) => <input id={id} className="input mono" value={key} onChange={(e) => setKey(e.target.value)} onBlur={commitKey} onKeyDown={(e) => e.key === 'Enter' && commitKey()} />}
      </Field>
      {stepIssues.length > 0 && (
        <ul className="panel-issues">
          {stepIssues.map((i) => (
            <li key={`${i.path}:${i.code}`} className={i.severity === 'error' ? 'issue-error' : 'issue-warning'}>
              {i.message} <span className="muted small mono">{i.path}</span>
            </li>
          ))}
        </ul>
      )}
      <Button variant="danger" onClick={() => deleteStep(step.key)}>
        Delete step
      </Button>
    </div>
  );
}
