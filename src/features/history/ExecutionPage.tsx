// Execution detail (Lightning record page): header with actions, highlights panel, then two views: the step
// timeline with trigger data, and the detailed run log (`?view=log`, so a link can open it directly).

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Activity, ChevronRight, ExternalLink, RotateCcw, Square, StepForward } from 'lucide-react';
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { ExecutionDetail } from '../../api/types';
import { StatusBadge } from '../../components/Badge';
import { Button, LinkButton } from '../../components/Button';
import { ErrorBox, Loading } from '../../components/Empty';
import { JsonView } from '../../components/JsonView';
import { MessageBar } from '../../components/MessageBar';
import { Tabs } from '../../components/Tabs';
import { useToast } from '../../components/Toast';
import { formatDuration, formatTime, isTerminal } from '../../lib/format';
import { MODE_LABELS } from './labels';
import { executionPollMs } from './polling';
import { RunLog } from './RunLog';
import { StepTimeline } from './StepTimeline';
import { can, useRole } from '../auth';

export function ExecutionPage() {
  const { executionId = '' } = useParams();
  const exec = useQuery({
    queryKey: qk.execution(executionId),
    queryFn: () => endpoints.execution(executionId),
    refetchInterval: (q) => executionPollMs(q.state.data?.status, q.state.status === 'error', q.state.data?.waiting_until ?? null, Date.now()),
    // Payloads can be large: not kept long after leaving the page.
    gcTime: 30_000,
  });
  if (exec.isLoading) return <Loading />;
  if (exec.error) {
    return (
      <div className="page">
        <ErrorBox message={errorMessage(exec.error)} />
      </div>
    );
  }
  if (!exec.data) return null;
  return <ExecutionView e={exec.data} />;
}

function Highlights({ e }: { e: ExecutionDetail }) {
  const facts: [string, string][] = [
    ['Started', formatTime(e.started_at ?? e.queued_at)],
    ['Finished', formatTime(e.finished_at)],
    ['Duration', formatDuration(e.started_at, e.finished_at)],
    ['Tasks', String(e.task_count)],
    ['Trigger', MODE_LABELS[e.mode] ?? e.mode],
  ];
  if (e.waiting_until) facts.push(['Waiting until', formatTime(e.waiting_until)]);
  return (
    <dl className="highlights">
      {facts.map(([label, value]) => (
        <div key={label} className="highlight">
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function ExecutionView({ e }: { e: ExecutionDetail }) {
  const canRerun = can(useRole(), 'execution.rerun');
  const qc = useQueryClient();
  const navigate = useNavigate();
  const toast = useToast();
  const [params, setParams] = useSearchParams();
  const view = params.get('view') === 'log' ? 'log' : 'steps';
  const refresh = () => void qc.invalidateQueries({ queryKey: qk.execution(e.id) });
  const retry = useMutation({ mutationFn: () => endpoints.retryExecution(e.id), onSuccess: refresh, onError: (err) => toast.show('error', errorMessage(err)) });
  const cancel = useMutation({ mutationFn: () => endpoints.cancelExecution(e.id), onSuccess: refresh, onError: (err) => toast.show('error', errorMessage(err)) });
  const rerun = useMutation({
    mutationFn: () => endpoints.rerunExecution(e.id),
    onSuccess: (res) => navigate(`/history/${res.execution_id}`),
    onError: (err) => toast.show('error', errorMessage(err)),
  });
  return (
    <div className="page">
      <nav className="breadcrumb" aria-label="Breadcrumb">
        <Link to="/history">History</Link>
        <ChevronRight size={12} aria-hidden="true" />
        <span className="mono">{e.id}</span>
      </nav>
      <section className="record-card">
        <div className="record-head">
          <span className="page-header-icon" style={{ background: '#ca5010' }} aria-hidden="true">
            <Activity size={22} />
          </span>
          <div className="page-header-text">
            <div className="page-header-label">Execution</div>
            <h1 className="page-header-title record-title">
              {e.flow_name} <StatusBadge status={e.status} />
            </h1>
          </div>
          <div className="page-header-actions">
            <LinkButton to={`/flows/${e.flow_id}`} icon={ExternalLink}>
              Open flow
            </LinkButton>
            {canRerun && e.status === 'failed' && (
              <Button icon={StepForward} onClick={() => retry.mutate()} disabled={retry.isPending} title="Continue from the failed step on the same version">
                Retry from failed step
              </Button>
            )}
            {canRerun && isTerminal(e.status) && (
              <Button icon={RotateCcw} onClick={() => rerun.mutate()} disabled={rerun.isPending} title="Start again with the same trigger data on the latest published version">
                Re-run
              </Button>
            )}
            {canRerun && !isTerminal(e.status) && (
              <Button variant="danger" icon={Square} onClick={() => cancel.mutate()} disabled={cancel.isPending}>
                Cancel run
              </Button>
            )}
          </div>
        </div>
        <Highlights e={e} />
      </section>
      {e.possible_duplicate && <MessageBar intent="warning">A worker stopped mid-step; one action may have run twice. Check the target app.</MessageBar>}
      {e.error_message && (
        <MessageBar intent="error" role="alert" title={e.error_code ?? 'Error'}>
          {e.failed_step_key && (
            <>
              in <span className="mono">{e.failed_step_key}</span>:{' '}
            </>
          )}
          {e.error_message}
        </MessageBar>
      )}
      {e.payloads_hidden && (
        <MessageBar intent="info" role="status">
          Run data is hidden for viewers: you see each step&apos;s status and timing, not its input, output or error text.
        </MessageBar>
      )}
      <Tabs
        className="exec-tabs"
        label="Run details"
        tabs={[
          { key: 'steps', label: 'Steps', count: new Set(e.steps.map((s) => s.step_key)).size },
          { key: 'log', label: 'Log' },
        ]}
        value={view}
        onChange={(v) => setParams(v === 'log' ? { view: 'log' } : {}, { replace: true })}
      />
      {view === 'log' ? (
        <section className="card" role="tabpanel" aria-label="Log">
          <RunLog executionId={e.id} status={e.status} waitingUntil={e.waiting_until} />
        </section>
      ) : (
        <div className="exec-grid" role="tabpanel" aria-label="Steps">
          <section className="card">
            <h2 className="card-title">Steps · {new Set(e.steps.map((s) => s.step_key)).size}</h2>
            <StepTimeline steps={e.steps} />
          </section>
          <section className="card">
            <h2 className="card-title">Trigger data</h2>
            {e.payloads_hidden ? <p className="muted">Hidden for viewers.</p> : <JsonView label="Payload" value={e.trigger_data} />}
            <p className="field-help">Queued {formatTime(e.queued_at)}</p>
          </section>
        </div>
      )}
    </div>
  );
}
