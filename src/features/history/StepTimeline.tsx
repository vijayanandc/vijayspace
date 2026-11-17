// Vertical step timeline: status icon per step, attempts grouped, collapsible input / output / error.

import type { LucideIcon } from 'lucide-react';
import { CircleCheck, CircleMinus, CircleX, Clock, LoaderCircle, TriangleAlert } from 'lucide-react';

import type { StepRun } from '../../api/types';
import { Badge, StatusBadge } from '../../components/Badge';
import { JsonView } from '../../components/JsonView';
import { formatDuration, formatTime } from '../../lib/format';
import { describeRun, groupRuns } from './timeline';

const ICONS: Record<StepRun['status'], LucideIcon> = {
  succeeded: CircleCheck,
  failed: CircleX,
  running: LoaderCircle,
  retry_scheduled: Clock,
  skipped: CircleMinus,
  unknown_outcome: TriangleAlert,
};

export function StepTimeline({ steps }: { steps: StepRun[] }) {
  const groups = groupRuns(steps);
  if (groups.length === 0) return <p className="field-help">No steps have run yet.</p>;
  return (
    <ol className="timeline">
      {groups.map((g) => {
        const Icon = ICONS[g.status];
        const last = g.runs[g.runs.length - 1];
        return (
          <li key={g.stepKey} className={`tl-item tl-${g.status}`}>
            <span className="tl-icon" aria-hidden="true">
              <Icon size={20} className={g.status === 'running' ? 'spin' : undefined} />
            </span>
            <div className="tl-card">
              <div className="tl-head">
                <strong className="mono tl-key">{g.stepKey}</strong>
                <span className="tl-kind">{g.kind}</span>
                <StatusBadge status={g.status} />
                {g.runs.length > 1 && <Badge tone="muted">{g.runs.length} attempts</Badge>}
                <span className="tl-time">{last ? formatDuration(last.started_at, last.finished_at) : ''}</span>
              </div>
              {g.runs.map((run) => (
                <div key={run.id} className="tl-run">
                  <div className="tl-run-meta">
                    {g.runs.length > 1 && <span className="tl-attempt">Attempt {run.attempt}</span>}
                    {g.runs.length > 1 && <StatusBadge status={run.status} />}
                    {describeRun(run) && <span>{describeRun(run)}</span>}
                    <span className="cell-muted">{formatTime(run.started_at)}</span>
                  </div>
                  {run.warnings.length > 0 && (
                    <div className="warning-text small">
                      <TriangleAlert size={12} aria-hidden="true" /> {run.warnings.join('; ')}
                    </div>
                  )}
                  {run.input !== null && <JsonView label="Input" value={run.input} collapsed />}
                  {run.output !== null && <JsonView label="Output" value={run.output} collapsed={run.kind !== 'action'} />}
                  {run.error !== null && <JsonView label="Error" value={run.error} />}
                </div>
              ))}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
