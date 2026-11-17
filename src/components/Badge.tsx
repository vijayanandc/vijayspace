import type { ReactNode } from 'react';

export type Tone = 'neutral' | 'success' | 'danger' | 'warning' | 'info' | 'muted';

export function Badge({ tone = 'neutral', children, title }: { tone?: Tone; children: ReactNode; title?: string }) {
  return (
    <span className={`badge badge-${tone}`} title={title}>
      {children}
    </span>
  );
}

const STATUS_TONES: Record<string, Tone> = {
  succeeded: 'success', active: 'success', failed: 'danger', error: 'danger', needs_reauth: 'warning',
  running: 'info', queued: 'muted', waiting: 'warning', cancelled: 'muted', retry_scheduled: 'warning',
  skipped: 'muted', unknown_outcome: 'warning',
};

export function StatusBadge({ status }: { status: string }) {
  return <Badge tone={STATUS_TONES[status] ?? 'neutral'}>{status.replace(/_/g, ' ')}</Badge>;
}
