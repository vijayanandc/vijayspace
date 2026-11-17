import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnections, useConnectors } from '../../api/hooks';
import type { Connection } from '../../api/types';
import { StatusBadge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { useToast } from '../../components/Toast';
import { formatTime } from '../../lib/format';
import { ConnectionDialog } from './ConnectionDialog';

export function ConnectionsPage() {
  const [creating, setCreating] = useState(false);
  const connections = useConnections();
  return (
    <div className="page">
      <div className="page-header">
        <h1>Connections</h1>
        <Button variant="primary" onClick={() => setCreating(true)}>
          + New connection
        </Button>
      </div>
      <p className="muted">Credentials are stored encrypted on the server and are never shown again.</p>
      {connections.isLoading && <Loading />}
      {connections.error && <ErrorBox message={errorMessage(connections.error)} />}
      {connections.data?.length === 0 && <EmptyState title="No connections yet">Connect Zoho CRM, Appblocks or Autodesk Forma to use them in flows.</EmptyState>}
      {connections.data && connections.data.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>App</th>
              <th>Status</th>
              <th>Updated</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {connections.data.map((c) => (
              <ConnectionRow key={c.id} connection={c} />
            ))}
          </tbody>
        </table>
      )}
      <ConnectionDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function ConnectionRow({ connection }: { connection: Connection }) {
  const { data: manifests = [] } = useConnectors();
  const manifest = manifests.find((m) => m.key === connection.connector);
  const qc = useQueryClient();
  const toast = useToast();
  const test = useMutation({
    mutationFn: () => endpoints.testConnection(connection.id),
    onSuccess: (res) => {
      toast.show(res.ok ? 'success' : 'error', res.ok ? `“${connection.name}” works` : `Test failed: ${res.error.message}`);
      void qc.invalidateQueries({ queryKey: qk.connections });
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: () => endpoints.deleteConnection(connection.id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: qk.connections }),
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  return (
    <tr>
      <td>
        <strong>{connection.name}</strong>
        <div className="muted small mono">{connection.id}</div>
      </td>
      <td>
        {manifest?.icon} {manifest?.name ?? connection.connector}
      </td>
      <td>
        <StatusBadge status={connection.status} />
      </td>
      <td className="muted small">{formatTime(connection.updated_at)}</td>
      <td className="row-actions">
        <Button variant="ghost" disabled={test.isPending} onClick={() => test.mutate()}>
          {test.isPending ? 'Testing…' : 'Test'}
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            if (window.confirm(`Delete connection “${connection.name}”?`)) remove.mutate();
          }}
        >
          Delete
        </Button>
      </td>
    </tr>
  );
}
