import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { ExecutionDetail } from '../../api/types';
import { Badge, StatusBadge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { ErrorBox, Loading } from '../../components/Empty';
import { JsonView } from '../../components/JsonView';
import { useToast } from '../../components/Toast';
import { formatDuration, formatTime, isTerminal } from '../../lib/format';
import { describeRun, groupRuns } from './timeline';

export function ExecutionPage() {
  const { executionId = '' } = useParams();
  const exec = useQuery({
    queryKey: qk.execution(executionId),
    queryFn: () => endpoints.execution(executionId),
    refetchInterval: (q) => (q.state.data && isTerminal(q.state.data.status) ? false : 2000),
  });
  if (exec.isLoading) return <Loading />;
  if (exec.error) return <ErrorBox message={errorMessage(exec.error)} />;
  if (!exec.data) return null;
  return <ExecutionView e={exec.data} />;
}

function ExecutionView({ e }: { e: ExecutionDetail }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const toast = useToast();
  const refresh = () => void qc.invalidateQueries({ queryKey: qk.execution(e.id) });
  const retry = useMutation({ mutationFn: () => endpoints.retryExecution(e.id), onSuccess: refresh, onError: (err) => toast.show('error', errorMessage(err)) });
  const cancel = useMutation({ mutationFn: () => endpoints.cancelExecution(e.id), onSuccess: refresh, onError: (err) => toast.show('error', errorMessage(err)) });
  const rerun = useMutation({
    mutationFn: () => endpoints.rerunExecution(e.id),
    onSuccess: (res) => navigate(`/history/${res.execution_id}`),
    onError: (err) => toast.show('error', errorMessage(err)),
  });
  const groups = groupRuns(e.steps);
  return (
    <div className="page">
      <div className="page-header">
        <div>
          <Link to="/history" className="muted">
            ← History
          </Link>
          <h1>
            {e.flow_name} <StatusBadge status={e.status} /> <Badge tone="muted">{e.mode}</Badge>
          </h1>
          <div className="muted small mono">{e.id}</div>
        </div>
        <div className="actions">
          <Link to={`/flows/${e.flow_id}`}>Open flow</Link>
          {e.status === 'failed' && (
            <Button onClick={() => retry.mutate()} disabled={retry.isPending} title="Continue from the failed step on the same version">
              Retry from failed step
            </Button>
          )}
          {isTerminal(e.status) && (
            <Button onClick={() => rerun.mutate()} disabled={rerun.isPending} title="Start again with the same trigger data on the latest published version">
              Re-run
            </Button>
          )}
          {!isTerminal(e.status) && (
            <Button variant="danger" onClick={() => cancel.mutate()} disabled={cancel.isPending}>
              Cancel
            </Button>
          )}
        </div>
      </div>
      <dl className="facts">
        <dt>Queued</dt>
        <dd>{formatTime(e.queued_at)}</dd>
        <dt>Started</dt>
        <dd>{formatTime(e.started_at)}</dd>
        <dt>Finished</dt>
        <dd>{formatTime(e.finished_at)}</dd>
        <dt>Duration</dt>
        <dd>{formatDuration(e.started_at, e.finished_at)}</dd>
        <dt>Tasks</dt>
        <dd>{e.task_count}</dd>
        {e.waiting_until && (
          <>
            <dt>Waiting until</dt>
            <dd>{formatTime(e.waiting_until)}</dd>
          </>
        )}
      </dl>
      {e.possible_duplicate && <div className="banner banner-warning">A worker stopped mid-step; one action may have run twice. Check the target app.</div>}
      {e.error_message && (
        <div className="error-box" role="alert">
          <strong>{e.error_code}</strong> {e.failed_step_key && <>in <span className="mono">{e.failed_step_key}</span>: </>}
          {e.error_message}
        </div>
      )}
      <JsonView label="Trigger data" value={e.trigger_data} collapsed />
      <h2>Steps</h2>
      {groups.length === 0 && <p className="muted">No steps have run yet.</p>}
      <ol className="timeline">
        {groups.map((g) => (
          <li key={g.stepKey} className={`timeline-item timeline-${g.status}`}>
            <div className="timeline-head">
              <StatusBadge status={g.status} /> <strong className="mono">{g.stepKey}</strong> <span className="muted small">{g.kind}</span>
              {g.runs.length > 1 && <Badge tone="muted">{g.runs.length} attempts</Badge>}
            </div>
            {g.runs.map((run) => (
              <div key={run.id} className="timeline-run">
                <div className="small">
                  {g.runs.length > 1 && <>Attempt {run.attempt} · </>}
                  <StatusBadge status={run.status} /> {describeRun(run)} <span className="muted">{formatTime(run.started_at)} · {formatDuration(run.started_at, run.finished_at)}</span>
                </div>
                {run.warnings.length > 0 && <div className="warning-text small">⚠ {run.warnings.join('; ')}</div>}
                {run.input !== null && <JsonView label="Input" value={run.input} collapsed />}
                {run.output !== null && <JsonView label="Output" value={run.output} collapsed={run.kind !== 'action'} />}
                {run.error !== null && <JsonView label="Error" value={run.error} />}
              </div>
            ))}
          </li>
        ))}
      </ol>
    </div>
  );
}
