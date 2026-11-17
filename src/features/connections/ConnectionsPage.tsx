// Connections: page header, command bar (search, app filter, refresh) and a sortable table with row actions.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Plug, PlugZap, Plus, RefreshCw, ShieldCheck, Trash2 } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnections, useConnectors } from '../../api/hooks';
import type { Connection, ConnectorManifest } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { StatusBadge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { OverflowMenu } from '../../components/Popover';
import { SearchBox } from '../../components/SearchBox';
import { SortHeader } from '../../components/Table';
import { useToast } from '../../components/Toast';
import { formatTime, relativeTime } from '../../lib/format';
import { type SortState, nextSort, sortRows } from '../../lib/sort';
import { ConnectionDialog } from './ConnectionDialog';

type Column = 'name' | 'app' | 'status' | 'created';

export function ConnectionsPage() {
  const [creating, setCreating] = useState(false);
  const [query, setQuery] = useState('');
  const [app, setApp] = useState('');
  const [sort, setSort] = useState<SortState<Column>>({ key: 'name', dir: 'asc' });
  const connections = useConnections();
  const { data: manifests = [] } = useConnectors();
  const appName = (key: string) => manifests.find((m) => m.key === key)?.name ?? key;
  const all = connections.data ?? [];
  const q = query.trim().toLowerCase();
  const visible = all.filter((c) => (!app || c.connector === app) && (!q || c.name.toLowerCase().includes(q) || appName(c.connector).toLowerCase().includes(q)));
  const rows = sortRows(visible, sort, (c, k) => (k === 'name' ? c.name : k === 'app' ? appName(c.connector) : k === 'status' ? c.status : c.created_at));
  const apps = [...new Set(all.map((c) => c.connector))];
  const onSort = (column: Column) => setSort((s) => nextSort(s, column));

  return (
    <div className="page">
      <section className="list-card">
        <PageHeader
          icon={Plug}
          tone="#0b7a75"
          label="Connections"
          title="Connections"
          meta={
            <>
              {all.length} item{all.length === 1 ? '' : 's'} · <ShieldCheck size={12} aria-hidden="true" /> Credentials are stored encrypted and never shown again
            </>
          }
          actions={
            <Button variant="primary" icon={Plus} onClick={() => setCreating(true)}>
              New connection
            </Button>
          }
        />
        <div className="command-bar">
          <SearchBox label="Search connections" placeholder="Search connections" value={query} onChange={setQuery} className="command-search" />
          <label className="cb-field">
            <span className="cb-label">App</span>
            <select className="input" value={app} onChange={(e) => setApp(e.target.value)}>
              <option value="">All apps</option>
              {apps.map((k) => (
                <option key={k} value={k}>
                  {appName(k)}
                </option>
              ))}
            </select>
          </label>
          <div className="command-spacer" />
          <button type="button" className="icon-btn" aria-label="Refresh" title="Refresh" onClick={() => void connections.refetch()}>
            <RefreshCw size={16} className={connections.isFetching ? 'spin' : undefined} aria-hidden="true" />
          </button>
        </div>
        {connections.isLoading && <Loading />}
        {connections.error && (
          <div className="card-pad">
            <ErrorBox message={errorMessage(connections.error)} />
          </div>
        )}
        {connections.data?.length === 0 && (
          <EmptyState title="No connections yet" action={<Button variant="primary" icon={Plus} onClick={() => setCreating(true)}>Add a connection</Button>}>
            Connect Zoho CRM, Appblocks or Autodesk Forma to use them in flows.
          </EmptyState>
        )}
        {all.length > 0 && visible.length === 0 && <EmptyState title="No connections match" compact>Try another search term or app.</EmptyState>}
        {rows.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <SortHeader label="Name" column="name" sort={sort} onSort={onSort} />
                <SortHeader label="App" column="app" sort={sort} onSort={onSort} />
                <SortHeader label="Status" column="status" sort={sort} onSort={onSort} />
                <SortHeader label="Created" column="created" sort={sort} onSort={onSort} />
                <th className="col-actions">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <ConnectionRow key={c.id} connection={c} manifest={manifests.find((m) => m.key === c.connector)} />
              ))}
            </tbody>
          </table>
        )}
      </section>
      <ConnectionDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}

function ConnectionRow({ connection, manifest }: { connection: Connection; manifest: ConnectorManifest | undefined }) {
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
    onSuccess: () => {
      toast.show('success', `Deleted “${connection.name}”`);
      void qc.invalidateQueries({ queryKey: qk.connections });
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const confirmDelete = () => {
    if (window.confirm(`Delete connection “${connection.name}”?`)) remove.mutate();
  };
  return (
    <tr>
      <td className="col-name">
        <div className="name-cell">
          <AppIcon app={connection.connector} name={manifest?.name ?? connection.connector} size={28} decorative />
          <div>
            <div className="row-title">{connection.name}</div>
            <div className="row-sub mono">{connection.id}</div>
          </div>
        </div>
      </td>
      <td>{manifest?.name ?? connection.connector}</td>
      <td>
        <StatusBadge status={connection.status} />
      </td>
      <td title={formatTime(connection.created_at)}>{relativeTime(connection.created_at)}</td>
      <td className="col-actions">
        <span className="row-actions">
          <Button size="sm" variant="ghost" icon={PlugZap} disabled={test.isPending} onClick={() => test.mutate()}>
            {test.isPending ? 'Testing…' : 'Test'}
          </Button>
          <OverflowMenu
            label={`More actions for ${connection.name}`}
            items={[
              { label: 'Test connection', icon: PlugZap, onSelect: () => test.mutate(), disabled: test.isPending },
              { label: 'Delete', icon: Trash2, danger: true, onSelect: confirmDelete },
            ]}
          />
        </span>
      </td>
    </tr>
  );
}
