// "My flows": page header, command bar (search, filter, view toggle, refresh) and the table or grid.

import { useQuery } from '@tanstack/react-query';
import { LayoutGrid, List, Plus, RefreshCw, Workflow } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { FlowSummary } from '../../api/types';
import { Button, LinkButton } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { SearchBox } from '../../components/SearchBox';
import { updatedAgo } from '../../lib/format';
import { useStoredChoice } from '../../lib/storage';
import { FlowGrid, FlowTable } from './FlowTable';
import { NewFlowDialog } from './NewFlowDialog';
import { can } from '../auth/permissions';
import { useRole } from '../auth/session';

type Filter = 'all' | 'on' | 'off' | 'draft';
const FILTERS: { key: Filter; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'on', label: 'On' },
  { key: 'off', label: 'Off' },
  { key: 'draft', label: 'Drafts' },
];
const VIEWS = ['list', 'grid'] as const;

function matches(f: FlowSummary, q: string, filter: Filter): boolean {
  if (filter === 'on' && !f.is_enabled) return false;
  if (filter === 'off' && f.is_enabled) return false;
  if (filter === 'draft' && f.published_version !== null) return false;
  return !q || f.name.toLowerCase().includes(q) || (f.description ?? '').toLowerCase().includes(q);
}

export function FlowsPage() {
  const canEdit = can(useRole(), 'flow.edit');
  const [creating, setCreating] = useState(false);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [view, setView] = useStoredChoice('weft.flows.view', 'list', VIEWS);
  const flows = useQuery({ queryKey: qk.flows, queryFn: endpoints.flows, refetchInterval: 15_000 });
  const all = flows.data ?? [];
  const q = query.trim().toLowerCase();
  const visible = all.filter((f) => matches(f, q, filter));
  const filtered = q !== '' || filter !== 'all';
  const count = filtered ? `${visible.length} of ${all.length} items` : `${all.length} item${all.length === 1 ? '' : 's'}`;

  return (
    <div className="page">
      <section className="list-card">
        <PageHeader
          icon={Workflow}
          label="Flows"
          title="My flows"
          meta={flows.data ? `${count} · Updated ${updatedAgo(flows.dataUpdatedAt)}` : 'Loading…'}
          actions={
            <>
              <LinkButton to="/recipes">Browse gallery</LinkButton>
              {canEdit && (
                <Button variant="primary" icon={Plus} onClick={() => setCreating(true)}>
                  New flow
                </Button>
              )}
            </>
          }
        />
        <div className="command-bar">
          <SearchBox label="Search this list" placeholder="Search this list" value={query} onChange={setQuery} className="command-search" />
          <div className="chip-group" role="group" aria-label="Filter flows">
            {FILTERS.map((f) => (
              <button key={f.key} type="button" className={`chip${filter === f.key ? ' chip-active' : ''}`} aria-pressed={filter === f.key} onClick={() => setFilter(f.key)}>
                {f.label}
              </button>
            ))}
          </div>
          <div className="command-spacer" />
          <div className="segmented" role="group" aria-label="View">
            <button type="button" className={`icon-btn${view === 'list' ? ' is-on' : ''}`} aria-label="List view" title="List view" aria-pressed={view === 'list'} onClick={() => setView('list')}>
              <List size={16} aria-hidden="true" />
            </button>
            <button type="button" className={`icon-btn${view === 'grid' ? ' is-on' : ''}`} aria-label="Grid view" title="Grid view" aria-pressed={view === 'grid'} onClick={() => setView('grid')}>
              <LayoutGrid size={16} aria-hidden="true" />
            </button>
          </div>
          <button type="button" className="icon-btn" aria-label="Refresh" title="Refresh" onClick={() => void flows.refetch()}>
            <RefreshCw size={16} className={flows.isFetching ? 'spin' : undefined} aria-hidden="true" />
          </button>
        </div>
        {flows.isLoading && <Loading />}
        {flows.error && (
          <div className="card-pad">
            <ErrorBox message={errorMessage(flows.error)} />
          </div>
        )}
        {flows.data && all.length === 0 && (
          <EmptyState title="Automate your first process" action={<LinkButton to="/recipes">Browse gallery</LinkButton>}>
            Connect Zoho CRM, Appblocks and Autodesk Forma. Start from scratch with <strong>New flow</strong>, or install a ready-made recipe from the gallery.
          </EmptyState>
        )}
        {flows.data && all.length > 0 && visible.length === 0 && (
          <EmptyState
            title="No flows match"
            compact
            action={
              <Button
                onClick={() => {
                  setQuery('');
                  setFilter('all');
                }}
              >
                Clear filters
              </Button>
            }
          >
            Try another search term or filter.
          </EmptyState>
        )}
        {visible.length > 0 && view === 'list' && <FlowTable flows={visible} />}
      </section>
      {visible.length > 0 && view === 'grid' && <FlowGrid flows={visible} />}
      <NewFlowDialog open={creating} onClose={() => setCreating(false)} />
    </div>
  );
}
