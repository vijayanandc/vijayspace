// Flows as a dense, sortable table (default view) or as a card grid.

import { useMemo, useState } from 'react';
import { Link } from 'react-router-dom';

import type { FlowSummary } from '../../api/types';
import { OverflowMenu } from '../../components/Popover';
import { Switch } from '../../components/Switch';
import { SortHeader } from '../../components/Table';
import { formatTime, relativeTime } from '../../lib/format';
import { type SortState, nextSort, sortRows } from '../../lib/sort';
import { AppChain, FlowStatus, LastRun } from './FlowParts';
import { useFlowActions } from './useFlowActions';
import { can, useRole } from '../auth';

type Column = 'name' | 'status' | 'last_run' | 'updated';

function sortValue(f: FlowSummary, key: Column): string | number | null {
  switch (key) {
    case 'name':
      return f.name;
    case 'status':
      return f.published_version ?? 0;
    case 'last_run':
      return f.last_execution?.queued_at ?? null;
    case 'updated':
      return f.updated_at;
  }
}

function FlowSwitch({ flow, toggle }: { flow: FlowSummary; toggle: { mutate: () => void; isPending: boolean } }) {
  const unpublished = flow.published_version === null;
  const editable = can(useRole(), 'flow.edit');
  return (
    <Switch
      checked={flow.is_enabled}
      disabled={!editable || toggle.isPending || (unpublished && !flow.is_enabled)}
      onChange={() => toggle.mutate()}
      label={`${flow.is_enabled ? 'Disable' : 'Enable'} ${flow.name}`}
      title={unpublished && !flow.is_enabled ? 'Publish the flow before switching it on' : undefined}
    />
  );
}

function FlowRow({ flow }: { flow: FlowSummary }) {
  const { toggle, items } = useFlowActions(flow);
  return (
    <tr>
      <td className="col-apps">
        <AppChain flow={flow} />
      </td>
      <td className="col-name">
        <Link to={`/flows/${flow.id}`} className="row-title">
          {flow.name}
        </Link>
        {flow.description && <div className="row-sub">{flow.description}</div>}
      </td>
      <td>
        <FlowStatus flow={flow} />
      </td>
      <td>
        <LastRun flow={flow} />
      </td>
      <td className="cell-muted" title={formatTime(flow.updated_at)}>
        {relativeTime(flow.updated_at)}
      </td>
      <td>
        <FlowSwitch flow={flow} toggle={toggle} />
      </td>
      <td className="col-actions">
        <OverflowMenu label={`More actions for ${flow.name}`} items={items} />
      </td>
    </tr>
  );
}

export function FlowTable({ flows }: { flows: FlowSummary[] }) {
  const [sort, setSort] = useState<SortState<Column>>({ key: 'updated', dir: 'desc' });
  const rows = useMemo(() => sortRows(flows, sort, sortValue), [flows, sort]);
  const onSort = (column: Column) => setSort((s) => nextSort(s, column));
  return (
    <table className="table">
      <thead>
        <tr>
          <th className="col-apps">Apps</th>
          <SortHeader label="Name" column="name" sort={sort} onSort={onSort} />
          <SortHeader label="Status" column="status" sort={sort} onSort={onSort} />
          <SortHeader label="Last run" column="last_run" sort={sort} onSort={onSort} />
          <SortHeader label="Modified" column="updated" sort={sort} onSort={onSort} />
          <th>Enabled</th>
          <th className="col-actions">
            <span className="sr-only">Actions</span>
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((f) => (
          <FlowRow key={f.id} flow={f} />
        ))}
      </tbody>
    </table>
  );
}

function FlowCard({ flow }: { flow: FlowSummary }) {
  const { toggle, items } = useFlowActions(flow);
  return (
    <article className="flow-card">
      <div className="flow-card-top">
        <AppChain flow={flow} size={32} />
        <OverflowMenu label={`More actions for ${flow.name}`} items={items} />
      </div>
      <Link to={`/flows/${flow.id}`} className="row-title flow-card-title">
        {flow.name}
      </Link>
      <p className="flow-card-desc">{flow.description || 'No description'}</p>
      <FlowStatus flow={flow} />
      <div className="flow-card-foot">
        <LastRun flow={flow} />
        <FlowSwitch flow={flow} toggle={toggle} />
      </div>
    </article>
  );
}

export function FlowGrid({ flows }: { flows: FlowSummary[] }) {
  return (
    <div className="flow-grid">
      {flows.map((f) => (
        <FlowCard key={f.id} flow={f} />
      ))}
    </div>
  );
}
