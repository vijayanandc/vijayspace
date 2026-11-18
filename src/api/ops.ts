// Platform operations contract (backend docs/api/README.md, `GET /api/v1/ops/status`, staff only): the live
// servers, the job queue's lag and the alerts staff should look at.

import { api } from './client';

export interface Instance {
  id: string;
  kind: 'api' | 'worker' | 'all';
  host: string;
  version: string;
  started_at: string;
  seen_at: string;
}

export interface QueueLag {
  queue: string;
  /** Jobs due now and not yet claimed. */
  ready: number;
  /** How long the oldest due job has waited. */
  lag_seconds: number;
}

export interface Alert {
  key: string;
  severity: 'critical' | 'warning';
  title: string;
  detail: string;
}

export interface OpsStatus {
  now: string;
  instances: Instance[];
  queues: QueueLag[];
  running_jobs: number;
  dead_jobs: number;
  alerts: Alert[];
}

export const ops = {
  status: () => api.get<OpsStatus>('/api/v1/ops/status'),
};
