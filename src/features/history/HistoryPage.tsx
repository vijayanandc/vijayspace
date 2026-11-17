import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import { Badge, StatusBadge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { formatDuration, formatTime } from '../../lib/format';

const STATUSES = ['', 'queued', 'running', 'waiting', 'succeeded', 'failed', 'cancelled'];

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
  return (
    <div className="page">
      <div className="page-header">
        <h1>History</h1>
      </div>
      <div className="filters">
        <label>
          Flow{' '}
          <select className="input" value={flowId} onChange={(e) => setFilter('flow_id', e.target.value)}>
            <option value="">All flows</option>
            {flows.data?.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Status{' '}
          <select className="input" value={status} onChange={(e) => setFilter('status', e.target.value)}>
            {STATUSES.map((s) => (
              <option key={s} value={s}>
                {s || 'Any status'}
              </option>
            ))}
          </select>
        </label>
        <span className="muted small">Refreshes every 5 s</span>
      </div>
      {executions.isLoading && <Loading />}
      {executions.error && <ErrorBox message={errorMessage(executions.error)} />}
      {!executions.isLoading && items.length === 0 && <EmptyState title="No executions">Runs appear here when flows are triggered or tested.</EmptyState>}
      {items.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Status</th>
              <th>Flow</th>
              <th>Mode</th>
              <th>Started</th>
              <th>Duration</th>
              <th>Tasks</th>
              <th>Error</th>
            </tr>
          </thead>
          <tbody>
            {items.map((e) => (
              <tr key={e.id}>
                <td>
                  <Link to={`/history/${e.id}`}>
                    <StatusBadge status={e.status} />
                  </Link>
                  {e.possible_duplicate && <Badge tone="warning" title="An action may have run twice after a crash">dup?</Badge>}
                </td>
                <td>
                  <Link to={`/history/${e.id}`}>{e.flow_name}</Link>
                </td>
                <td>
                  <Badge tone="muted">{e.mode}</Badge>
                </td>
                <td className="small">{formatTime(e.started_at ?? e.queued_at)}</td>
                <td className="small">{formatDuration(e.started_at, e.finished_at)}</td>
                <td>{e.task_count}</td>
                <td className="small error-text">{e.error_message ? `${e.failed_step_key ? `${e.failed_step_key}: ` : ''}${e.error_message}` : ''}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {executions.hasNextPage && (
        <Button onClick={() => void executions.fetchNextPage()} disabled={executions.isFetchingNextPage}>
          {executions.isFetchingNextPage ? 'Loading…' : 'Load more'}
        </Button>
      )}
    </div>
  );
}
