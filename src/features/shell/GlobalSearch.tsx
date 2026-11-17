// Global header search: finds flows by name or description and opens them in the builder.

import { useQuery } from '@tanstack/react-query';
import { type KeyboardEvent, useId, useRef, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';

import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { FlowSummary } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { useDismiss } from '../../components/Popover';
import { SearchBox } from '../../components/SearchBox';

function flowMeta(f: FlowSummary): string {
  const state = f.published_version ? `Published v${f.published_version}` : 'Draft';
  return `${state} · ${f.is_enabled ? 'On' : 'Off'}`;
}

export function GlobalSearch() {
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const ref = useRef<HTMLDivElement>(null);
  const listId = useId();
  const navigate = useNavigate();
  useDismiss(ref, open, () => setOpen(false));
  const flows = useQuery({ queryKey: qk.flows, queryFn: endpoints.flows, enabled: open });
  const q = query.trim().toLowerCase();
  const results = (flows.data ?? [])
    .filter((f) => !q || f.name.toLowerCase().includes(q) || (f.description ?? '').toLowerCase().includes(q))
    .slice(0, 8);

  const go = (id: string) => {
    setOpen(false);
    setQuery('');
    navigate(`/flows/${id}`);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      setOpen(true);
      const delta = e.key === 'ArrowDown' ? 1 : -1;
      setActive((i) => (results.length ? (i + delta + results.length) % results.length : 0));
    } else if (e.key === 'Enter') {
      const pick = results[active];
      if (pick) go(pick.id);
    }
  };

  return (
    <div className="global-search" ref={ref} role="search">
      <SearchBox
        label="Search flows"
        placeholder="Search flows"
        value={query}
        onChange={(v) => {
          setQuery(v);
          setOpen(true);
          setActive(0);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={onKeyDown}
        role="combobox"
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        autoComplete="off"
      />
      {open && (
        <div className="search-results popover" id={listId} role="listbox" aria-label="Flows">
          <div className="search-results-head">{q ? 'Matching flows' : 'Recent flows'}</div>
          {flows.isLoading && <div className="search-empty">Loading…</div>}
          {results.map((f, i) => (
            <div
              key={f.id}
              role="option"
              aria-selected={i === active}
              className={`search-result${i === active ? ' search-result-active' : ''}`}
              onMouseDown={(e) => e.preventDefault()}
              onMouseEnter={() => setActive(i)}
              onClick={() => go(f.id)}
            >
              <AppIcon app={`trigger:${f.trigger_type}`} size={24} decorative />
              <span className="search-result-text">
                <span className="search-result-name">{f.name}</span>
                <span className="search-result-meta">{flowMeta(f)}</span>
              </span>
            </div>
          ))}
          {flows.data && results.length === 0 && <div className="search-empty">No flows match “{query.trim()}”.</div>}
          <Link to="/flows" className="search-results-foot" onClick={() => setOpen(false)}>
            View all flows
          </Link>
        </div>
      )}
    </div>
  );
}
