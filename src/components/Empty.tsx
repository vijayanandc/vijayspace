import type { ReactNode } from 'react';

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <h3>{title}</h3>
      {children}
    </div>
  );
}

export function Loading({ label = 'Loading…' }: { label?: string }) {
  return <div className="loading" role="status">{label}</div>;
}

export function ErrorBox({ message }: { message: string }) {
  return <div className="error-box" role="alert">{message}</div>;
}
