// Side panel header (icon tile, kind, inline step name, close) and the selected node's issues.

import { CircleX, TriangleAlert, X } from 'lucide-react';

import type { Step } from '../../../api/types';
import { AppIcon } from '../../../components/AppIcon';
import { useBuilder } from '../BuilderContext';
import type { NodeLabel } from '../canvas/labels';
import { updateStep } from '../../../domain/tree';

export function PanelHeader({ label, step, onClose }: { label: NodeLabel; step?: Step; onClose: () => void }) {
  const { def, apply } = useBuilder();
  const kicker = label.iconName && label.iconName !== label.kind ? `${label.kind} · ${label.iconName}` : label.kind;
  return (
    <div className="panel-head">
      <AppIcon app={label.icon} name={label.iconName} size={36} decorative />
      <div className="panel-head-text">
        <div className="panel-kicker">{kicker}</div>
        {step ? (
          <input
            className="title-input panel-title"
            aria-label="Step name"
            title="Rename step"
            value={step.name ?? ''}
            placeholder={label.title}
            onChange={(e) => apply(updateStep(def, step.key, (s) => ({ ...s, name: e.target.value })), `name:${step.key}`)}
          />
        ) : (
          <h2 className="panel-title">{label.title}</h2>
        )}
      </div>
      <button type="button" className="icon-btn" aria-label="Close panel" title="Close" onClick={onClose}>
        <X size={18} aria-hidden="true" />
      </button>
    </div>
  );
}

/** Validation issues of one node (trigger or step), shown at the top of its Configure tab. */
export function NodeIssues({ id }: { id: string }) {
  const { issues } = useBuilder();
  const list = issues.get(id) ?? [];
  if (list.length === 0) return null;
  return (
    <ul className="panel-issues">
      {list.map((i) => {
        const Icon = i.severity === 'error' ? CircleX : TriangleAlert;
        return (
          <li key={`${i.path}:${i.code}`} className={i.severity === 'error' ? 'issue-error' : 'issue-warning'}>
            <Icon size={14} aria-hidden="true" />
            <span>
              {i.message} <span className="issue-path">{i.path}</span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}
