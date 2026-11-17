// Validation issues drawer under the canvas; click an issue to select its step.

import { ChevronRight, CircleCheck, CircleX, TriangleAlert, X } from 'lucide-react';

import type { Issue } from '../../api/types';
import { Badge } from '../../components/Badge';
import { useBuilder } from './BuilderContext';
import { nodeForPointer } from './model/paths';

export function IssuesPanel({ issues, onClose }: { issues: Issue[]; onClose: () => void }) {
  const { def, select } = useBuilder();
  const errors = issues.filter((i) => i.severity === 'error').length;
  const warnings = issues.length - errors;
  return (
    <section className="issues-panel" aria-label="Validation issues">
      <div className="issues-header">
        <strong>Issues</strong>
        <Badge tone={errors ? 'danger' : 'muted'}>
          {errors} error{errors === 1 ? '' : 's'}
        </Badge>
        <Badge tone={warnings ? 'warning' : 'muted'}>
          {warnings} warning{warnings === 1 ? '' : 's'}
        </Badge>
        <div className="toolbar-spacer" />
        <button type="button" className="icon-btn icon-btn-sm" aria-label="Close issues" onClick={onClose}>
          <X size={16} aria-hidden="true" />
        </button>
      </div>
      {issues.length === 0 && (
        <p className="issues-ok">
          <CircleCheck size={16} aria-hidden="true" /> No issues. The flow can be published.
        </p>
      )}
      <ul>
        {issues.map((issue) => {
          const Icon = issue.severity === 'error' ? CircleX : TriangleAlert;
          return (
            <li key={`${issue.path}:${issue.code}:${issue.message}`}>
              <button type="button" className={`issue ${issue.severity === 'error' ? 'issue-error' : 'issue-warning'}`} onClick={() => select(nodeForPointer(def, issue.path) ?? null)}>
                <Icon size={16} className="issue-icon" aria-hidden="true" />
                <span className="issue-message">{issue.message}</span>
                <span className="issue-path">{issue.path}</span>
                <ChevronRight size={14} className="issue-go" aria-hidden="true" />
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
