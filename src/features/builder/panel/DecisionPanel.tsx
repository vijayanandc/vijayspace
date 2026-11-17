// "Configure" tab of a decision: ordered branches with conditions, plus the otherwise path.

import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';

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
      <p className="field-help panel-intro">Branches are checked in order; the first that matches runs. Otherwise runs when none match.</p>
      {step.branches.map((b, i) => (
        <section key={b.key} className="branch-card">
          <div className="branch-header">
            <span className="branch-num" aria-hidden="true">
              {i + 1}
            </span>
            <input
              className="input"
              aria-label={`Branch ${i + 1} name`}
              value={b.name ?? ''}
              placeholder={b.key}
              onChange={(e) => apply(updateBranch(def, step.key, b.key, (x) => ({ ...x, name: e.target.value })), `branch-name:${step.key}:${b.key}`)}
            />
            <button type="button" className="icon-btn icon-btn-sm" aria-label="Move branch up" title="Move up" disabled={i === 0} onClick={() => apply(moveBranch(def, step.key, b.key, -1))}>
              <ArrowUp size={14} aria-hidden="true" />
            </button>
            <button type="button" className="icon-btn icon-btn-sm" aria-label="Move branch down" title="Move down" disabled={i === step.branches.length - 1} onClick={() => apply(moveBranch(def, step.key, b.key, 1))}>
              <ArrowDown size={14} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="icon-btn icon-btn-sm"
              aria-label={`Remove branch ${b.name ?? b.key}`}
              title="Remove branch"
              disabled={step.branches.length <= 1}
              onClick={() => {
                if (b.steps.length === 0 || window.confirm(`Remove “${b.name ?? b.key}” and its ${b.steps.length} step(s)?`)) apply(removeBranch(def, step.key, b.key));
              }}
            >
              <Trash2 size={14} aria-hidden="true" />
            </button>
          </div>
          <ConditionBuilder
            value={b.condition}
            variables={variables}
            onChange={(condition) => apply(updateBranch(def, step.key, b.key, (x) => ({ ...x, condition })), `cond:${step.key}:${b.key}`)}
          />
        </section>
      ))}
      <button type="button" className="btn btn-secondary btn-sm" onClick={() => apply(addBranch(def, step.key))}>
        <Plus size={14} aria-hidden="true" />
        Add branch
      </button>
      <section className="branch-card branch-otherwise">
        <span className="branch-num" aria-hidden="true">
          ∗
        </span>
        <span>
          <strong>Otherwise</strong> · {step.otherwise.length} step{step.otherwise.length === 1 ? '' : 's'} when no branch matches
        </span>
      </section>
    </div>
  );
}
