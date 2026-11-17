// Lightning-style page header: icon tile, object label, title, meta line and right-aligned actions.

import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';

interface Props {
  icon: LucideIcon;
  /** Tile color; defaults to the brand blue. */
  tone?: string;
  /** Small object label above the title, e.g. "Flows". */
  label: string;
  title: ReactNode;
  meta?: ReactNode;
  actions?: ReactNode;
}

export function PageHeader({ icon: Icon, tone, label, title, meta, actions }: Props) {
  return (
    <header className="page-header">
      <span className="page-header-icon" style={tone ? { background: tone } : undefined} aria-hidden="true">
        <Icon size={22} />
      </span>
      <div className="page-header-text">
        <div className="page-header-label">{label}</div>
        <h1 className="page-header-title">{title}</h1>
        {meta && <div className="page-header-meta">{meta}</div>}
      </div>
      {actions && <div className="page-header-actions">{actions}</div>}
    </header>
  );
}
