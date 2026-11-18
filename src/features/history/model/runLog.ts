// Pure logic of the run log view: filtering by kind and step, time offsets from the run's start, detail rows of a
// line, and the log as plain text for copying.

import type { JsonValue, LogEntry } from '../../../api/types';

export type LogFilter = 'all' | 'problems' | 'http';

/** The lines shown for a filter and, optionally, one step. */
export function filterLog(entries: LogEntry[], filter: LogFilter, step: string | null): LogEntry[] {
  return entries.filter((e) => {
    if (step !== null && e.step_key !== step) return false;
    if (filter === 'problems') return e.level !== 'info';
    if (filter === 'http') return e.kind === 'http';
    return true;
  });
}

/** How many lines each filter would show. */
export function filterCounts(entries: LogEntry[]): Record<LogFilter, number> {
  return {
    all: entries.length,
    problems: entries.filter((e) => e.level !== 'info').length,
    http: entries.filter((e) => e.kind === 'http').length,
  };
}

/** Step keys in the order they first appear. */
export function stepKeysOf(entries: LogEntry[]): string[] {
  const keys: string[] = [];
  for (const e of entries) if (e.step_key !== null && !keys.includes(e.step_key)) keys.push(e.step_key);
  return keys;
}

/** `+0 ms`, `+182 ms`, `+2.4 s`, `+3 min 5 s` after `start`. */
export function offsetLabel(at: string, start: string): string {
  const ms = Math.max(0, Date.parse(at) - Date.parse(start));
  if (!Number.isFinite(ms)) return '';
  if (ms < 1000) return `+${ms} ms`;
  if (ms < 60_000) return `+${(ms / 1000).toFixed(1)} s`;
  return `+${Math.floor(ms / 60_000)} min ${Math.floor((ms % 60_000) / 1000)} s`;
}

/** `512 B`, `1.2 KB`, `3.4 MB`. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

const LABELS: Record<string, string> = {
  method: 'Method',
  url: 'URL',
  status: 'Status',
  error: 'Error',
  duration_ms: 'Time',
  request_bytes: 'Sent',
  response_bytes: 'Received',
  reason: 'Reason',
  until: 'Until',
  code: 'Code',
  retry_at: 'Retry at',
  next_attempt: 'Next attempt',
  not_logged: 'Calls not logged',
  possible_duplicate: 'Possible duplicate',
  rerun_of: 'Re-run of',
};

/** Keys holding a time (shown in local time by the view). */
export const TIME_KEYS = new Set(['until', 'retry_at']);

function text(value: JsonValue): string {
  if (value === null) return '';
  if (typeof value === 'string') return value;
  return JSON.stringify(value);
}

/** Known keys first, in reading order (stored JSON comes back with its keys re-ordered). */
const ORDER = Object.keys(LABELS);
const rank = (key: string) => (ORDER.includes(key) ? ORDER.indexOf(key) : ORDER.length);

/** A line's details as label/value rows (empty values left out; the error answer is shown on its own). */
export function detailRows(entry: LogEntry): { key: string; label: string; value: string }[] {
  if (!entry.details) return [];
  return Object.entries(entry.details)
    .filter(([key, value]) => key !== 'response_excerpt' && value !== null && value !== '')
    .sort(([a], [b]) => rank(a) - rank(b))
    .map(([key, value]) => {
      let shown = text(value);
      if (key === 'duration_ms' && typeof value === 'number') shown = `${value} ms`;
      if ((key === 'request_bytes' || key === 'response_bytes') && typeof value === 'number') shown = formatBytes(value);
      return { key, label: LABELS[key] ?? key, value: shown };
    });
}

/** The start of the app's answer, when it refused. */
export function excerptOf(entry: LogEntry): string | null {
  const v = entry.details?.['response_excerpt'];
  return typeof v === 'string' && v !== '' ? v : null;
}

/** The lines as plain text (one per line: time, level, step and attempt, message). */
export function logAsText(entries: LogEntry[]): string {
  return entries
    .map((e) => {
      const where = e.step_key ? ` [${e.step_key}${e.attempt !== null ? ` #${e.attempt}` : ''}]` : '';
      const excerpt = excerptOf(e);
      return `${e.at} ${e.level.toUpperCase().padEnd(5)}${where} ${e.message}${excerpt ? `\n    ${excerpt}` : ''}`;
    })
    .join('\n');
}
