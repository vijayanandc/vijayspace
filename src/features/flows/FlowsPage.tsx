import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Link } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { FlowSummary } from '../../api/types';
import { Badge, StatusBadge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { useToast } from '../../components/Toast';
import { relativeTime } from '../../lib/format';
import { NewFlowDialog } from './NewFlowDialog';

export function FlowsPage() {
  const [creating, setCreating] = useState(false);
  const flows = useQuery({ queryKey: qk.flows, queryFn: endpoints.flows, refetchInterval: 15_000 });
  return (
    <div className="page">
      <div className="page-header">
        <h1>Flows</h1>
        <Button variant="primary" onClick={() => setCreating(true)}>
          + New flow
        </Button>
      </div>
      {flows.isLoading && <Loading />}
      {flows.error && <ErrorBox message={errorMessage(flows.error)} />}
      {flows.data && flows.data.length === 0 && (
        <EmptyState title="No flows yet">
          <p>Create a flow from scratch or install a recipe.</p>
          <Link to="/recipes">Browse recipes →</Link>
        </EmptyState>
      )}
      {flows.data && flows.data.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Trigger</th>
              <th>Version</th>
              <th>Last run</th>
              <th>Enabled</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {flows.data.map((f) => (
              <FlowRow key={f.id} flow={f} />
            ))}
          </tbody>
        </table>
      )}
      <NewFlowDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function FlowRow({ flow }: { flow: FlowSummary }) {
  const qc = useQueryClient();
  const toast = useToast();
  const toggle = useMutation({
    mutationFn: () => (flow.is_enabled ? endpoints.disable(flow.id) : endpoints.enable(flow.id)),
    onSuccess: () => void qc.invalidateQueries({ queryKey: qk.flows }),
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: () => endpoints.deleteFlow(flow.id),
    onSuccess: () => {
      toast.show('success', `Deleted “${flow.name}”`);
      void qc.invalidateQueries({ queryKey: qk.flows });
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  return (
    <tr>
      <td>
        <Link to={`/flows/${flow.id}`} className="strong-link">
          {flow.name}
        </Link>
        {flow.description && <div className="muted small">{flow.description}</div>}
      </td>
      <td>
        <Badge tone="neutral">{flow.trigger_type}</Badge>
      </td>
      <td>
        {flow.published_version ? `v${flow.published_version}` : <span className="muted">draft</span>}{' '}
        {flow.has_unpublished_changes && <Badge tone="warning">unpublished changes</Badge>}
      </td>
      <td>
        {flow.last_execution ? (
          <Link to={`/history/${flow.last_execution.id}`}>
            <StatusBadge status={flow.last_execution.status} /> <span className="muted small">{relativeTime(flow.last_execution.queued_at)}</span>
          </Link>
        ) : (
          <span className="muted">—</span>
        )}
      </td>
      <td>
        <label className="switch">
          <input
            type="checkbox"
            checked={flow.is_enabled}
            disabled={toggle.isPending}
            onChange={() => toggle.mutate()}
            aria-label={`${flow.is_enabled ? 'Disable' : 'Enable'} ${flow.name}`}
          />
          <span className="switch-track" />
        </label>
      </td>
      <td className="row-actions">
        <Button
          variant="ghost"
          onClick={() => {
            if (window.confirm(`Delete “${flow.name}”? Its history is kept until retention expires.`)) remove.mutate();
          }}
        >
          Delete
        </Button>
      </td>
    </tr>
  );
}
