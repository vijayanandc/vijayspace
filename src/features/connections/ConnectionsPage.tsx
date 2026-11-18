// Connections: page header, command bar (search, app filter, refresh) and a sortable table with row actions.

import { Plug, Plus, RefreshCw, ShieldCheck } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { useConnectionUsage, useConnections, useConnectors } from '../../api/hooks';
import type { Connection } from '../../api/types';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { SearchBox } from '../../components/SearchBox';
import { SortHeader } from '../../components/Table';
import { type SortState, nextSort, sortRows } from '../../lib/sort';
import { ConnectionDialog } from './ConnectionDialog';
import { ConnectionRow } from './ConnectionRow';
import { LimitsDialog } from './LimitsDialog';
import { can } from '../auth/permissions';
import { useRole } from '../auth/session';

type Column = 'name' | 'app' | 'status' | 'today' | 'created';

export function ConnectionsPage() {
  const canCreate = can(useRole(), 'connection.create');
  const [creating, setCreating] = useState(false);
  const [query, setQuery] = useState('');
  const [app, setApp] = useState('');
  const [sort, setSort] = useState<SortState<Column>>({ key: 'name', dir: 'asc' });
  const [limitsFor, setLimitsFor] = useState<Connection | null>(null);
  const connections = useConnections();
  const usage = useConnectionUsage();
  const { data: manifests = [] } = useConnectors();
  const appName = (key: string) => manifests.find((m) => m.key === key)?.name ?? key;
  const usageOf = (id: string) => usage.data?.find((u) => u.connection_id === id);
  const all = connections.data ?? [];
  const q = query.trim().toLowerCase();
  const visible = all.filter((c) => (!app || c.connector === app) && (!q || c.name.toLowerCase().includes(q) || appName(c.connector).toLowerCase().includes(q)));
  const rows = sortRows(visible, sort, (c, k) =>
    k === 'name' ? c.name : k === 'app' ? appName(c.connector) : k === 'status' ? c.status : k === 'today' ? (usageOf(c.id)?.requests ?? 0) : c.created_at,
  );
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
            canCreate && (
              <Button variant="primary" icon={Plus} onClick={() => setCreating(true)}>
                New connection
              </Button>
            )
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
          <button
            type="button"
            className="icon-btn"
            aria-label="Refresh"
            title="Refresh"
            onClick={() => {
              void connections.refetch();
              void usage.refetch();
            }}
          >
            <RefreshCw size={16} className={connections.isFetching || usage.isFetching ? 'spin' : undefined} aria-hidden="true" />
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
                <SortHeader label="Today" column="today" sort={sort} onSort={onSort} />
                <SortHeader label="Created" column="created" sort={sort} onSort={onSort} />
                <th className="col-actions">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <ConnectionRow key={c.id} connection={c} manifest={manifests.find((m) => m.key === c.connector)} usage={usageOf(c.id)} onLimits={() => setLimitsFor(c)} />
              ))}
            </tbody>
          </table>
        )}
      </section>
      <ConnectionDialog open={creating} onClose={() => setCreating(false)} />
      <LimitsDialog connection={limitsFor} appName={limitsFor ? appName(limitsFor.connector) : ''} onClose={() => setLimitsFor(null)} />
    </div>
  );
}
