// Empty states (with a small inline illustration), loading indicator and error message bars.

import { Loader2 } from 'lucide-react';
import type { ReactNode } from 'react';

import { MessageBar } from './MessageBar';

/** A neutral illustration: two connected cards and a "+" (no third-party artwork). */
function EmptyArt() {
  return (
    <svg className="empty-art" width="132" height="92" viewBox="0 0 132 92" aria-hidden="true">
      <ellipse cx="66" cy="84" rx="52" ry="5" fill="#e8e8e8" />
      <rect x="8" y="14" width="50" height="34" rx="6" fill="#ffffff" stroke="#d1d1d1" />
      <rect x="16" y="22" width="12" height="12" rx="3" fill="#cfe4fa" />
      <rect x="32" y="23" width="18" height="4" rx="2" fill="#d1d1d1" />
      <rect x="32" y="30" width="12" height="4" rx="2" fill="#e6e6e6" />
      <path d="M58 31 H74" stroke="#b4d6fa" strokeWidth="2" strokeDasharray="3 3" />
      <rect x="74" y="14" width="50" height="34" rx="6" fill="#ffffff" stroke="#d1d1d1" />
      <rect x="82" y="22" width="12" height="12" rx="3" fill="#e0d7f2" />
      <rect x="98" y="23" width="18" height="4" rx="2" fill="#d1d1d1" />
      <rect x="98" y="30" width="12" height="4" rx="2" fill="#e6e6e6" />
      <circle cx="66" cy="64" r="12" fill="#0f6cbd" />
      <path d="M66 58 V70 M60 64 H72" stroke="#ffffff" strokeWidth="2.4" strokeLinecap="round" />
    </svg>
  );
}

interface EmptyProps {
  title: string;
  children?: ReactNode;
  /** Primary (and optional secondary) actions. */
  action?: ReactNode;
  compact?: boolean;
}

export function EmptyState({ title, children, action, compact = false }: EmptyProps) {
  return (
    <div className={`empty${compact ? ' empty-compact' : ''}`}>
      {!compact && <EmptyArt />}
      <h3 className="empty-title">{title}</h3>
      {children && <div className="empty-text">{children}</div>}
      {action && <div className="empty-actions">{action}</div>}
    </div>
  );
}

export function Loading({ label = 'Loading…' }: { label?: string }) {
  return (
    <div className="loading" role="status">
      <Loader2 size={16} className="spin" aria-hidden="true" />
      {label}
    </div>
  );
}

export function ErrorBox({ message }: { message: string }) {
  return (
    <MessageBar intent="error" role="alert">
      {message}
    </MessageBar>
  );
}
