// Fluent-style message bar: an intent icon, the message and optional actions.

import { CircleCheck, CircleX, Info, TriangleAlert } from 'lucide-react';
import type { ReactNode } from 'react';

export type Intent = 'info' | 'success' | 'warning' | 'error';

const ICONS = { info: Info, success: CircleCheck, warning: TriangleAlert, error: CircleX };

interface Props {
  intent?: Intent;
  title?: string;
  children?: ReactNode;
  actions?: ReactNode;
  role?: 'alert' | 'status';
  className?: string;
}

export function MessageBar({ intent = 'info', title, children, actions, role, className }: Props) {
  const Icon = ICONS[intent];
  return (
    <div className={`message-bar message-${intent}${className ? ` ${className}` : ''}`} role={role}>
      <Icon size={16} className="message-icon" aria-hidden="true" />
      <div className="message-text">
        {title && <strong className="message-title">{title} </strong>}
        {children}
      </div>
      {actions && <div className="message-actions">{actions}</div>}
    </div>
  );
}
