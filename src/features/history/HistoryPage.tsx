// Execution history: filters in a command bar, a dense table of runs and "Load more".

import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { ChevronRight, History, RefreshCw } from 'lucide-react';
import { Link, useSearchParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import { Badge, StatusBadge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { formatDuration, formatTime, relativeTime } from '../../lib/format';
import { MODE_LABELS } from './labels';

const STATUSES: { value: string; label: string }[] = [
  { value: '', label: 'All' },
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'running', label: 'Running' },
  { value: 'waiting', label: 'Waiting' },
  { value: 'queued', label: 'Queued' },
  { value: 'cancelled', label: 'Cancelled' },
];

export function HistoryPage() {
  const [params, setParams] = useSearchParams();
  const flowId = params.get('flow_id') ?? '';
  const status = params.get('status') ?? '';
  const flows = useQuery({ queryKey: qk.flows, queryFn: endpoints.flows });
  const filters = { flow_id: flowId || undefined, status: status || undefined };
  const executions = useInfiniteQuery({
    queryKey: qk.executions(filters),
    queryFn: ({ pageParam }) => endpoints.executions({ ...filters, limit: 50, cursor: pageParam }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    refetchInterval: 5000,
  });
  const setFilter = (key: string, value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    setParams(next);
  };
  const items = executions.data?.pages.flatMap((p) => p.items) ?? [];
  const count = `${items.length}${executions.hasNextPage ? '+' : ''} run${items.length === 1 ? '' : 's'}`;

  return (
    <div className="page">
      <section className="list-card">
        <PageHeader
          icon={History}
          tone="#ca5010"
          label="History"
          title="Execution history"
          meta={executions.data ? `${count} · Refreshes every 5 seconds` : 'Loading…'}
          actions={
            <Button icon={RefreshCw} onClick={() => void executions.refetch()} disabled={executions.isFetching}>
              Refresh
            </Button>
          }
        />
        <div className="command-bar">
          <label className="cb-field">
            <span className="cb-label">Flow</span>
            <select className="input" value={flowId} onChange={(e) => setFilter('flow_id', e.target.value)}>
              <option value="">All flows</option>
              {flows.data?.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
          <div className="chip-group" role="group" aria-label="Status">
            {STATUSES.map((s) => (
              <button key={s.value || 'all'} type="button" className={`chip${status === s.value ? ' chip-active' : ''}`} aria-pressed={status === s.value} onClick={() => setFilter('status', s.value)}>
                {s.label}
              </button>
            ))}
          </div>
        </div>
        {executions.isLoading && <Loading />}
        {executions.error && (
          <div className="card-pad">
            <ErrorBox message={errorMessage(executions.error)} />
          </div>
        )}
        {!executions.isLoading && !executions.error && items.length === 0 && (
          <EmptyState title="No runs yet">Runs appear here when flows are triggered, tested or run manually.</EmptyState>
        )}
        {items.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>Status</th>
                <th>Flow</th>
                <th>Run type</th>
                <th>Started</th>
                <th className="num">Duration</th>
                <th className="num">Tasks</th>
                <th>Details</th>
                <th className="col-actions">
                  <span className="sr-only">Open</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {items.map((e) => (
                <tr key={e.id}>
                  <td>
                    <span className="pill-stack">
                      <StatusBadge status={e.status} />
                      {e.possible_duplicate && (
                        <Badge tone="warning" title="An action may have run twice after a crash">
                          Possible duplicate
                        </Badge>
                      )}
                    </span>
                  </td>
                  <td className="col-name">
                    <Link to={`/history/${e.id}`} className="row-title">
                      {e.flow_name}
                    </Link>
                    <div className="row-sub mono">{e.id}</div>
                  </td>
                  <td>
                    <Badge tone="muted">{MODE_LABELS[e.mode] ?? e.mode}</Badge>
                  </td>
                  <td title={formatTime(e.started_at ?? e.queued_at)}>
                    {relativeTime(e.started_at ?? e.queued_at)}
                    <div className="row-sub">{formatTime(e.started_at ?? e.queued_at)}</div>
                  </td>
                  <td className="num">{formatDuration(e.started_at, e.finished_at)}</td>
                  <td className="num">{e.task_count}</td>
                  <td className="cell-error">{e.error_message ? `${e.failed_step_key ? `${e.failed_step_key}: ` : ''}${e.error_message}` : ''}</td>
                  <td className="col-actions">
                    <Link to={`/history/${e.id}`} className="icon-btn icon-btn-sm" aria-label={`Open run of ${e.flow_name}`} title="Open run">
                      <ChevronRight size={16} aria-hidden="true" />
                    </Link>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {executions.hasNextPage && (
          <div className="table-foot">
            <Button onClick={() => void executions.fetchNextPage()} disabled={executions.isFetchingNextPage}>
              {executions.isFetchingNextPage ? 'Loading…' : 'Load more'}
            </Button>
          </div>
        )}
      </section>
    </div>
  );
}
