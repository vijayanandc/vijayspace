import type { Issue } from '../../api/types';
import { useBuilder } from './BuilderContext';
import { nodeForPointer } from './model/paths';

export function IssuesPanel({ issues, onClose }: { issues: Issue[]; onClose: () => void }) {
  const { def, select } = useBuilder();
  return (
    <section className="issues-panel" aria-label="Validation issues">
      <div className="issues-header">
        <strong>
          {issues.filter((i) => i.severity === 'error').length} errors · {issues.filter((i) => i.severity === 'warning').length} warnings
        </strong>
        <button type="button" className="icon-btn" aria-label="Close issues" onClick={onClose}>
          ×
        </button>
      </div>
      {issues.length === 0 && <p className="muted">No issues. The flow can be published.</p>}
      <ul>
        {issues.map((issue) => (
          <li key={`${issue.path}:${issue.code}:${issue.message}`}>
            <button type="button" className={`issue ${issue.severity === 'error' ? 'issue-error' : 'issue-warning'}`} onClick={() => select(nodeForPointer(def, issue.path) ?? null)}>
              <span aria-hidden="true">{issue.severity === 'error' ? '⛔' : '⚠'}</span> {issue.message}
              <span className="muted small mono"> {issue.path}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
