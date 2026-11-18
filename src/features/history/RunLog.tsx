// The run's detailed log: one chronological list from the server (run events, step attempts, every HTTP call,
// limit holds, retries), filterable by problems, HTTP calls and step, with each line's details and a copy button.

import { useQuery } from '@tanstack/react-query';
import type { LucideIcon } from 'lucide-react';
import { CircleAlert, CircleX, Copy, Info } from 'lucide-react';
import { useId, useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { LogEntry, LogLevel } from '../../api/types';
import { Button } from '../../components/Button';
import { ErrorBox, Loading } from '../../components/Empty';
import { MessageBar } from '../../components/MessageBar';
import { useToast } from '../../components/Toast';
import { COPY_FAILED, copyText } from '../../lib/clipboard';
import { formatTime } from '../../lib/format';
import type { LogFilter } from './model/runLog';
import { TIME_KEYS, detailRows, excerptOf, filterCounts, filterLog, logAsText, offsetLabel, stepKeysOf } from './model/runLog';
import { executionPollMs } from './polling';

const LEVEL_ICONS: Record<LogLevel, LucideIcon> = { info: Info, warn: CircleAlert, error: CircleX };
const LEVEL_TEXT: Record<LogLevel, string> = { info: 'Info', warn: 'Warning', error: 'Error' };
const FILTERS: { key: LogFilter; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'problems', label: 'Problems' },
  { key: 'http', label: 'HTTP calls' },
];

interface Props {
  executionId: string;
  status: string;
  waitingUntil: string | null;
}

export function RunLog({ executionId, status, waitingUntil }: Props) {
  const log = useQuery({
    queryKey: qk.executionLog(executionId),
    queryFn: () => endpoints.executionLog(executionId),
    refetchInterval: (q) => executionPollMs(status, q.state.status === 'error', waitingUntil, Date.now()),
    gcTime: 30_000,
  });
  const [filter, setFilter] = useState<LogFilter>('all');
  const [step, setStep] = useState<string | null>(null);
  const toast = useToast();
  if (log.isLoading) return <Loading />;
  if (log.error) return <ErrorBox message={errorMessage(log.error)} />;
  const entries = log.data?.entries ?? [];
  const shown = filterLog(entries, filter, step);
  const counts = filterCounts(entries);
  const start = entries[0]?.at ?? '';
  const copy = () =>
    copyText(logAsText(shown)).then(
      () => toast.show('success', `Copied ${shown.length} line${shown.length === 1 ? '' : 's'}`),
      () => toast.show('error', COPY_FAILED),
    );
  return (
    <div className="run-log">
      <div className="run-log-toolbar">
        <div className="chip-group" role="group" aria-label="Show">
          {FILTERS.map((f) => (
            <button key={f.key} type="button" className={`chip${filter === f.key ? ' chip-active' : ''}`} aria-pressed={filter === f.key} onClick={() => setFilter(f.key)}>
              {f.label} <span className="tab-count">{counts[f.key]}</span>
            </button>
          ))}
        </div>
        <label className="run-log-step">
          <span>Step</span>
          <select value={step ?? ''} onChange={(e) => setStep(e.target.value === '' ? null : e.target.value)}>
            <option value="">All steps</option>
            {stepKeysOf(entries).map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
        </label>
        <Button icon={Copy} onClick={() => void copy()} disabled={shown.length === 0}>
          Copy log
        </Button>
      </div>
      {log.data?.truncated && <MessageBar intent="info">This run has a very long log: the first 2,000 lines are shown.</MessageBar>}
      {shown.length === 0 ? (
        <p className="field-help">{entries.length === 0 ? 'Nothing has been logged yet.' : 'No lines match this filter.'}</p>
      ) : (
        <ol className="run-log-lines" aria-label="Log lines">
          {shown.map((e, i) => (
            // Lines have no ID; their position in this list is stable while the filter is unchanged.
            <LogLine key={`${e.at}-${i}`} entry={e} start={start} />
          ))}
        </ol>
      )}
    </div>
  );
}

function LogLine({ entry, start }: { entry: LogEntry; start: string }) {
  const [open, setOpen] = useState(false);
  const detailsId = useId();
  const Icon = LEVEL_ICONS[entry.level];
  const rows = detailRows(entry);
  const excerpt = excerptOf(entry);
  const expandable = rows.length > 0 || excerpt !== null;
  return (
    <li className={`rl-line rl-${entry.level}`}>
      <div className="rl-row">
        <span className="rl-offset" title={formatTime(entry.at)}>
          {offsetLabel(entry.at, start)}
        </span>
        <span className="rl-level">
          <Icon size={14} aria-hidden="true" />
          <span className="sr-only">{LEVEL_TEXT[entry.level]}</span>
        </span>
        {entry.step_key && (
          <span className="rl-step mono">
            {entry.step_key}
            {entry.attempt !== null && <span className="rl-attempt"> #{entry.attempt}</span>}
          </span>
        )}
        <span className={entry.kind === 'http' ? 'rl-msg mono' : 'rl-msg'}>{entry.message}</span>
        {expandable && (
          <button type="button" className="link-btn rl-toggle" aria-expanded={open} aria-controls={detailsId} onClick={() => setOpen((v) => !v)}>
            {open ? 'Hide details' : 'Details'}
          </button>
        )}
      </div>
      {open && (
        <div id={detailsId} className="rl-details">
          {rows.length > 0 && (
            <dl className="rl-facts">
              {rows.map((r) => (
                <div key={r.key}>
                  <dt>{r.label}</dt>
                  <dd className={r.key === 'url' ? 'mono' : undefined}>{TIME_KEYS.has(r.key) ? formatTime(r.value) : r.value}</dd>
                </div>
              ))}
            </dl>
          )}
          {excerpt !== null && (
            <figure className="rl-excerpt">
              <figcaption>The app answered</figcaption>
              <pre>{excerpt}</pre>
            </figure>
          )}
        </div>
      )}
    </li>
  );
}
