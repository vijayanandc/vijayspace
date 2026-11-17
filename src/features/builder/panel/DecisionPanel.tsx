import type { DecisionStep } from '../../../api/types';
import { useBuilder } from '../BuilderContext';
import { ConditionBuilder } from '../condition/ConditionBuilder';
import { addBranch, moveBranch, removeBranch, updateBranch } from '../model/tree';
import { buildVariableTree } from '../model/variables';

export function DecisionPanel({ step }: { step: DecisionStep }) {
  const { def, apply, manifests, flow } = useBuilder();
  const variables = buildVariableTree(def, step.key, manifests, flow.trigger?.sample);
  return (
    <div className="panel-body">
      <p className="muted small">Branches are checked in order; the first that matches runs. Otherwise runs when none match.</p>
      {step.branches.map((b, i) => (
        <section key={b.key} className="branch-card">
          <div className="branch-header">
            <input
              className="input"
              aria-label={`Branch ${i + 1} name`}
              value={b.name ?? ''}
              placeholder={b.key}
              onChange={(e) => apply(updateBranch(def, step.key, b.key, (x) => ({ ...x, name: e.target.value })), `branch-name:${step.key}:${b.key}`)}
            />
            <button type="button" className="icon-btn" aria-label="Move branch up" disabled={i === 0} onClick={() => apply(moveBranch(def, step.key, b.key, -1))}>
              ↑
            </button>
            <button type="button" className="icon-btn" aria-label="Move branch down" disabled={i === step.branches.length - 1} onClick={() => apply(moveBranch(def, step.key, b.key, 1))}>
              ↓
            </button>
            <button
              type="button"
              className="icon-btn"
              aria-label={`Remove branch ${b.name ?? b.key}`}
              disabled={step.branches.length <= 1}
              onClick={() => {
                if (b.steps.length === 0 || window.confirm(`Remove “${b.name ?? b.key}” and its ${b.steps.length} step(s)?`)) apply(removeBranch(def, step.key, b.key));
              }}
            >
              ×
            </button>
          </div>
          <ConditionBuilder
            value={b.condition}
            variables={variables}
            onChange={(condition) => apply(updateBranch(def, step.key, b.key, (x) => ({ ...x, condition })), `cond:${step.key}:${b.key}`)}
          />
        </section>
      ))}
      <button type="button" className="link-btn" onClick={() => apply(addBranch(def, step.key))}>
        + Add branch
      </button>
      <section className="branch-card muted">Otherwise: {step.otherwise.length} step(s)</section>
    </div>
  );
}
