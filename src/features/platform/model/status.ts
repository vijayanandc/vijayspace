// Pure logic of the platform page: server kinds in words, queue lag levels, alert order and a one-line summary.

/** The parts of `api/ops.ts` types this logic reads (the model layer does not import the api layer). */
interface Instance {
  kind: 'api' | 'worker' | 'all';
}
interface Alert {
  severity: 'critical' | 'warning';
  title: string;
}
interface OpsStatus {
  instances: Instance[];
  alerts: Alert[];
}

/** Lag above this is slow (amber), above the alert threshold it is behind (red). */
const SLOW_SECS = 60;
const BEHIND_SECS = 300;

export function kindLabel(kind: Instance['kind']): string {
  if (kind === 'api') return 'API';
  if (kind === 'worker') return 'Worker';
  return 'API + worker';
}

export function lagLevel(seconds: number): 'ok' | 'slow' | 'behind' {
  if (seconds > BEHIND_SECS) return 'behind';
  if (seconds > SLOW_SECS) return 'slow';
  return 'ok';
}

/** `12 s`, `4 min`, `2 h 5 min`. */
export function waitLabel(seconds: number): string {
  if (seconds < 60) return `${Math.max(0, Math.round(seconds))} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

/** Critical first, then by title. */
export function sortAlerts<A extends Alert>(alerts: A[]): A[] {
  return [...alerts].sort((a, b) => (a.severity === b.severity ? a.title.localeCompare(b.title) : a.severity === 'critical' ? -1 : 1));
}

/** Servers that run jobs (workers and `serve`). */
export function workerCount(instances: Instance[]): number {
  return instances.filter((i) => i.kind !== 'api').length;
}

/** "3 servers (2 workers) · 1 alert" (a `serve` process counts as a worker). */
export function summary(status: OpsStatus): string {
  const n = status.instances.length;
  const w = workerCount(status.instances);
  const alerts = status.alerts.length;
  const servers = `${n} server${n === 1 ? '' : 's'} (${w} worker${w === 1 ? '' : 's'})`;
  return `${servers} · ${alerts === 0 ? 'no alerts' : `${alerts} alert${alerts === 1 ? '' : 's'}`}`;
}
