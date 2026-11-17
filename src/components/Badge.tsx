// Pill badges and status pills (executions, step runs, connections).

import type { ReactNode } from 'react';

export type Tone = 'neutral' | 'success' | 'danger' | 'warning' | 'info' | 'muted' | 'brand';

interface BadgeProps {
  tone?: Tone;
  children: ReactNode;
  title?: string;
  /** Leading status dot. */
  dot?: boolean;
}

export function Badge({ tone = 'neutral', children, title, dot = false }: BadgeProps) {
  return (
    <span className={`badge badge-${tone}${dot ? ' badge-dot' : ''}`} title={title}>
      {children}
    </span>
  );
}

const STATUS: Record<string, { tone: Tone; label: string }> = {
  succeeded: { tone: 'success', label: 'Succeeded' },
  active: { tone: 'success', label: 'Active' },
  failed: { tone: 'danger', label: 'Failed' },
  error: { tone: 'danger', label: 'Error' },
  needs_reauth: { tone: 'warning', label: 'Needs re-auth' },
  running: { tone: 'info', label: 'Running' },
  queued: { tone: 'muted', label: 'Queued' },
  waiting: { tone: 'warning', label: 'Waiting' },
  cancelled: { tone: 'muted', label: 'Cancelled' },
  retry_scheduled: { tone: 'warning', label: 'Retry scheduled' },
  skipped: { tone: 'muted', label: 'Skipped' },
  unknown_outcome: { tone: 'warning', label: 'Unknown outcome' },
};

export function statusLabel(status: string): string {
  const label = STATUS[status]?.label;
  if (label) return label;
  const text = status.replace(/_/g, ' ');
  return text.charAt(0).toUpperCase() + text.slice(1);
}

export function statusTone(status: string): Tone {
  return STATUS[status]?.tone ?? 'neutral';
}

export function StatusBadge({ status }: { status: string }) {
  return (
    <Badge tone={statusTone(status)} dot>
      {statusLabel(status)}
    </Badge>
  );
}
