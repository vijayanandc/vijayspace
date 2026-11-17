// Click-to-open popovers and the row overflow menu (⋯). Outside click and Escape close them.

import type { LucideIcon } from 'lucide-react';
import { MoreHorizontal } from 'lucide-react';
import { type ReactNode, type RefObject, useEffect, useRef, useState } from 'react';

/** Calls `close` on a mousedown outside `ref` or on Escape while `open`. */
export function useDismiss(ref: RefObject<HTMLElement>, open: boolean, close: () => void): void {
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: MouseEvent) => {
      if (e.target instanceof Node && !ref.current?.contains(e.target)) closeRef.current();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeRef.current();
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [ref, open]);
}

interface PopoverProps {
  /** Renders the trigger button; receives the open state and a toggle. */
  trigger: (open: boolean, toggle: () => void) => ReactNode;
  children: (close: () => void) => ReactNode;
  align?: 'left' | 'right';
  className?: string;
}

export function Popover({ trigger, children, align = 'right', className }: PopoverProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, open, () => setOpen(false));
  return (
    <div className="popover-anchor" ref={ref}>
      {trigger(open, () => setOpen((v) => !v))}
      {open && <div className={`popover popover-${align}${className ? ` ${className}` : ''}`}>{children(() => setOpen(false))}</div>}
    </div>
  );
}

export interface MenuItem {
  label: string;
  icon?: LucideIcon;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
}

/** Row overflow menu: a ⋯ icon button that opens a list of actions. */
export function OverflowMenu({ label, items }: { label: string; items: MenuItem[] }) {
  return (
    <Popover
      trigger={(open, toggle) => (
        <button type="button" className="icon-btn icon-btn-sm" aria-label={label} title="More actions" aria-haspopup="menu" aria-expanded={open} onClick={toggle}>
          <MoreHorizontal size={16} aria-hidden="true" />
        </button>
      )}
    >
      {(close) => (
        <div role="menu" className="menu">
          {items.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.label}
                type="button"
                role="menuitem"
                className={`menu-item${item.danger ? ' menu-item-danger' : ''}`}
                disabled={item.disabled}
                onClick={() => {
                  close();
                  item.onSelect();
                }}
              >
                {Icon && <Icon size={16} aria-hidden="true" />}
                {item.label}
              </button>
            );
          })}
        </div>
      )}
    </Popover>
  );
}
