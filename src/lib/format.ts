// Display helpers: times, durations and relative phrases.

export function formatTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

export function formatDuration(start: string | null | undefined, end: string | null | undefined): string {
  if (!start || !end) return '—';
  const ms = new Date(end).getTime() - new Date(start).getTime();
  if (!Number.isFinite(ms) || ms < 0) return '—';
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const minutes = Math.floor(ms / 60_000);
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

export function relativeTime(iso: string | null | undefined, now: number = Date.now()): string {
  if (!iso) return '—';
  const diff = now - new Date(iso).getTime();
  if (!Number.isFinite(diff)) return iso;
  const s = Math.round(diff / 1000);
  if (s < 0) return 'in the future';
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86_400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86_400)}d ago`;
}

export function isTerminal(status: string): boolean {
  return status === 'succeeded' || status === 'failed' || status === 'cancelled';
}

/** Long relative phrase for page meta lines ("Updated a few seconds ago"). */
export function updatedAgo(at: number, now: number = Date.now()): string {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 45) return 'a few seconds ago';
  if (s < 90) return 'a minute ago';
  if (s < 2700) return `${Math.round(s / 60)} minutes ago`;
  if (s < 5400) return 'an hour ago';
  if (s < 86_400) return `${Math.round(s / 3600)} hours ago`;
  return `${Math.round(s / 86_400)} days ago`;
}

/** Splits help text on `backticks` into plain and code parts (an unpaired backtick stays as text). */
export function inlineCodeParts(text: string): { code: boolean; text: string }[] {
  const parts = text.split('`');
  if (parts.length % 2 === 0) return [{ code: false, text }];
  return parts.map((t, i) => ({ code: i % 2 === 1, text: t })).filter((p) => p.text !== '');
}
