// Shared pieces of the flows list: app-icon chain, publish status pills and the last-run cell.

import { ChevronRight } from 'lucide-react';
import { Fragment } from 'react';
import { Link } from 'react-router-dom';

import { useConnectors } from '../../api/hooks';
import type { FlowSummary } from '../../api/types';
import { AppIcon, TILE_NAMES } from '../../components/AppIcon';
import { Badge, StatusBadge } from '../../components/Badge';
import { formatTime, relativeTime } from '../../lib/format';
import { splitChain, summaryChain } from './appChain';

/** Trigger app → action apps, from the list row's `apps`. */
export function AppChain({ flow, size = 24 }: { flow: FlowSummary; size?: number }) {
  const { data: manifests = [] } = useConnectors();
  const chain = summaryChain(flow);
  const { shown, extra } = splitChain(chain);
  const name = (key: string) => manifests.find((m) => m.key === key)?.name ?? TILE_NAMES[key] ?? key;
  return (
    <div className="app-chain">
      {shown.map((key, i) => (
        <Fragment key={key}>
          {i > 0 && <ChevronRight size={12} className="app-chain-arrow" aria-hidden="true" />}
          <AppIcon app={key} name={name(key)} size={size} />
        </Fragment>
      ))}
      {extra > 0 && (
        <span className="app-chain-more" title={chain.slice(shown.length).map(name).join(', ')}>
          +{extra}
        </span>
      )}
    </div>
  );
}

export function FlowStatus({ flow }: { flow: FlowSummary }) {
  return (
    <span className="pill-stack">
      {flow.published_version ? (
        <Badge tone="info" dot>
          Published v{flow.published_version}
        </Badge>
      ) : (
        <Badge tone="muted" dot>
          Draft
        </Badge>
      )}
      {flow.published_version && flow.has_unpublished_changes && <Badge tone="warning">Unpublished changes</Badge>}
    </span>
  );
}

export function LastRun({ flow }: { flow: FlowSummary }) {
  const last = flow.last_execution;
  if (!last) return <span className="cell-muted">Never run</span>;
  return (
    <Link to={`/history/${last.id}`} className="last-run" title={formatTime(last.queued_at)}>
      <StatusBadge status={last.status} />
      <span className="cell-muted">{relativeTime(last.queued_at)}</span>
    </Link>
  );
}
