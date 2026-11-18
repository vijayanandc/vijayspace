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

function isTopmost(el: HTMLElement | null): boolean {
  const open = document.querySelectorAll('[role="dialog"][aria-modal="true"]');
  return el !== null && open[open.length - 1] === el;
}

const TABBABLE =
  'a[href], button:not(:disabled), input:not(:disabled):not([type=hidden]), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

/** Keeps Tab and Shift+Tab inside `dialog` (the page behind a modal is not reachable). */
function trapTab(e: KeyboardEvent, dialog: HTMLElement): void {
  const items = [...dialog.querySelectorAll<HTMLElement>(TABBABLE)];
  const first = items[0];
  const last = items[items.length - 1];
  if (!first || !last) return;
  const inside = dialog.contains(document.activeElement);
  if (e.shiftKey && (document.activeElement === first || !inside)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && (document.activeElement === last || !inside)) {
    e.preventDefault();
    first.focus();
  }
}

/** Open modal dialogs: the app behind them is inert (not focusable, not read) while there is at least one. */
let openDialogs = 0;
function setAppInert(inert: boolean): void {
  const app = document.getElementById('root');
  if (app) app.inert = inert;
}

/**
 * Modal dialog (portalled to <body> so it covers the whole app): Escape closes the topmost one, Tab stays inside it,
 * the app behind is inert, and focus moves into the body and returns afterwards (to the element that opened it while
 * it is still on the page).
 */
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
      // Nested dialogs are portalled in opening order, so only the last one in the document is on top.
      if (!isTopmost(ref.current) || !ref.current) return;
      if (e.key === 'Escape') closeRef.current();
      else if (e.key === 'Tab') trapTab(e, ref.current);
    };
    document.addEventListener('keydown', onKey);
    openDialogs += 1;
    setAppInert(true);
    return () => {
      document.removeEventListener('keydown', onKey);
      openDialogs -= 1;
      if (openDialogs === 0) setAppInert(false);
      if (previous?.isConnected) previous.focus();
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
