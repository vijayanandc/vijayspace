// Server kinds, queue lag levels, alert order and the summary line.

import { describe, expect, it } from 'vitest';

import { kindLabel, lagLevel, sortAlerts, summary, waitLabel } from './status';

const instance = (kind: 'api' | 'worker' | 'all') => ({ id: kind, kind, host: 'h', version: '0.1.0', started_at: '', seen_at: '' });

describe('platform status', () => {
  it('names server kinds and grades queue lag', () => {
    expect([kindLabel('api'), kindLabel('worker'), kindLabel('all')]).toEqual(['API', 'Worker', 'API + worker']);
    expect([lagLevel(5), lagLevel(90), lagLevel(301)]).toEqual(['ok', 'slow', 'behind']);
    expect([waitLabel(12), waitLabel(240), waitLabel(7500)]).toEqual(['12 s', '4 min', '2 h 5 min']);
  });

  it('puts critical alerts first and summarises the platform', () => {
    const alerts = sortAlerts([
      { key: 'b', severity: 'warning', title: 'B', detail: '' },
      { key: 'n', severity: 'critical', title: 'No worker is running', detail: '' },
      { key: 'a', severity: 'warning', title: 'A', detail: '' },
    ]);
    expect(alerts.map((a) => a.key)).toEqual(['n', 'a', 'b']);
    const status = { now: '', instances: [instance('api'), instance('worker'), instance('worker')], queues: [], running_jobs: 0, dead_jobs: 0, alerts };
    expect(summary(status)).toBe('3 servers (2 workers) · 3 alerts');
    expect(summary({ ...status, instances: [instance('all')], alerts: [] })).toBe('1 server (1 worker) · no alerts');
  });
});
