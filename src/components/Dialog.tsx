// Lightning-style modal: header with title and close X, scrollable body, grey footer with right-aligned buttons.

import { X } from 'lucide-react';
import { type ReactNode, useEffect, useId, useRef } from 'react';
import { createPortal } from 'react-dom';

interface Props {
  title: string;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
  /** Optional line under the title. */
  subtitle?: ReactNode;
}

/** Modal dialog (portalled to <body> so it covers the whole app): Escape closes, focus moves into the body and returns afterwards. */
export function Dialog({ title, open, onClose, children, footer, wide, subtitle }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  // Keep the latest onClose without re-running the focus effect on every parent render.
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    if (!open) return undefined;
    const previous = document.activeElement as HTMLElement | null;
    const focusable = 'input:not([type=radio]), input[type=radio]:checked, select, textarea, button';
    const body = ref.current?.querySelector('.dialog-body');
    const first = body?.querySelector<HTMLElement>(focusable) ?? ref.current?.querySelector<HTMLElement>(focusable);
    first?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeRef.current();
    };
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('keydown', onKey);
      previous?.focus();
    };
  }, [open]);
  if (!open) return null;
  return createPortal(
    <div className="dialog-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={ref} className={`dialog${wide ? ' dialog-wide' : ''}`} role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <header className="dialog-header">
          <div className="dialog-heading">
            <h2 id={titleId}>{title}</h2>
            {subtitle && <div className="dialog-subtitle">{subtitle}</div>}
          </div>
          <button type="button" className="icon-btn" aria-label="Close" onClick={onClose}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <div className="dialog-body">{children}</div>
        {footer && <footer className="dialog-footer">{footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}
