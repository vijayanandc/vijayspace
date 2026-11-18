// Settings → Platform (staff only): the alerts staff should act on, the live servers and the job queue's lag,
// refreshed every 15 seconds. When "waiting" keeps growing, another `weft worker` is started.

import { useQuery } from '@tanstack/react-query';
import { CircleAlert, CircleCheck, CircleX, RefreshCw, Server } from 'lucide-react';
import { Link } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { ops } from '../../api/ops';
import type { OpsStatus } from '../../api/ops';
import { Badge } from '../../components/Badge';
import { ErrorBox, Loading } from '../../components/Empty';
import { MessageBar } from '../../components/MessageBar';
import { PageHeader } from '../../components/PageHeader';
import { formatTime, relativeTime } from '../../lib/format';
import { useSession } from '../auth';
import { kindLabel, lagLevel, sortAlerts, summary, waitLabel } from './model/status';

const REFRESH_MS = 15_000;
const LAG_TONE = { ok: 'success', slow: 'warning', behind: 'danger' } as const;

export function PlatformPage() {
  const staff = useSession()?.me.staff ?? true;
  const status = useQuery({
    queryKey: ['ops-status'],
    queryFn: ops.status,
    enabled: staff,
    refetchInterval: (q) => (q.state.status === 'error' ? false : REFRESH_MS),
  });
  return (
    <div className="page">
      <nav className="breadcrumb" aria-label="Breadcrumb">
        <Link to="/settings">Settings</Link>
      </nav>
      <section className="list-card">
        <PageHeader
          icon={Server}
          tone="#616161"
          label="Settings"
          title="Platform"
          meta={status.data ? `${summary(status.data)} · updated ${formatTime(status.data.now)}` : 'Servers, queues and alerts'}
          actions={
            <button type="button" className="icon-btn" aria-label="Refresh" title="Refresh" onClick={() => void status.refetch()}>
              <RefreshCw size={16} className={status.isFetching ? 'spin' : undefined} aria-hidden="true" />
            </button>
          }
        />
      </section>
      {!staff && <MessageBar intent="info">Only the operator&apos;s staff (owners and admins of the default organisation) see the platform.</MessageBar>}
      {status.isLoading && <Loading />}
      {status.error && <ErrorBox message={errorMessage(status.error)} />}
      {status.data && <PlatformView s={status.data} />}
    </div>
  );
}

function PlatformView({ s }: { s: OpsStatus }) {
  const alerts = sortAlerts(s.alerts);
  const now = Date.parse(s.now);
  return (
    <>
      <section className="card" aria-labelledby="alerts-title">
        <h2 id="alerts-title" className="card-title">
          Alerts · {alerts.length}
        </h2>
        {alerts.length === 0 ? (
          <p className="ops-ok">
            <CircleCheck size={16} aria-hidden="true" /> No alerts: servers and queues are healthy.
          </p>
        ) : (
          <ul className="ops-alerts">
            {alerts.map((a) => (
              <li key={a.key} className={`ops-alert ops-${a.severity}`}>
                {a.severity === 'critical' ? <CircleX size={16} aria-hidden="true" /> : <CircleAlert size={16} aria-hidden="true" />}
                <div>
                  <strong>
                    <span className="sr-only">{a.severity === 'critical' ? 'Critical: ' : 'Warning: '}</span>
                    {a.title}
                  </strong>
                  <p className="muted small">{a.detail}</p>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
      <div className="exec-grid">
        <section className="card" aria-labelledby="servers-title">
          <h2 id="servers-title" className="card-title">
            Servers · {s.instances.length}
          </h2>
          <table className="table">
            <thead>
              <tr>
                <th>Host</th>
                <th>Runs</th>
                <th>Version</th>
                <th>Started</th>
                <th>Last seen</th>
              </tr>
            </thead>
            <tbody>
              {s.instances.map((i) => (
                <tr key={i.id}>
                  <td className="mono">{i.host}</td>
                  <td>{kindLabel(i.kind)}</td>
                  <td>{i.version}</td>
                  <td title={formatTime(i.started_at)}>{relativeTime(i.started_at, now)}</td>
                  <td title={formatTime(i.seen_at)}>{relativeTime(i.seen_at, now)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {s.instances.length === 0 && <p className="field-help">No server has reported in the last 45 seconds.</p>}
        </section>
        <section className="card" aria-labelledby="queues-title">
          <h2 id="queues-title" className="card-title">Job queue</h2>
          <dl className="highlights ops-counts">
            <div className="highlight">
              <dt>Running</dt>
              <dd>{s.running_jobs}</dd>
            </div>
            <div className="highlight">
              <dt>Dead</dt>
              <dd>{s.dead_jobs}</dd>
            </div>
          </dl>
          {s.queues.length === 0 ? (
            <p className="field-help">Nothing is waiting.</p>
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th>Queue</th>
                  <th>Due now</th>
                  <th>Oldest has waited</th>
                </tr>
              </thead>
              <tbody>
                {s.queues.map((q) => (
                  <tr key={q.queue}>
                    <td className="mono">{q.queue}</td>
                    <td>{q.ready}</td>
                    <td>
                      <Badge tone={LAG_TONE[lagLevel(q.lag_seconds)]}>{waitLabel(q.lag_seconds)}</Badge>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <p className="field-help">If the wait keeps growing, start another worker: runs are shared between workers automatically.</p>
        </section>
      </div>
    </>
  );
}
