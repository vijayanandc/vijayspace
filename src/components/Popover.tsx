// Click-to-open popovers and the row overflow menu (⋯). Outside click, Escape and tabbing away close them; closing
// from inside (Escape, choosing an item) returns focus to the trigger, so a dialog opened from a menu gives it back.
// The menu follows the ARIA menu pattern: focus moves to the first item, arrows/Home/End move between items.

import type { LucideIcon } from 'lucide-react';
import { MoreHorizontal } from 'lucide-react';
import { type KeyboardEvent as ReactKeyboardEvent, type ReactNode, type RefObject, useEffect, useRef, useState } from 'react';

/** Calls `close` on a mousedown outside `ref` or on Escape while `open` (`refocus`: Escape, not an outside click). */
export function useDismiss(ref: RefObject<HTMLElement>, open: boolean, close: (refocus: boolean) => void): void {
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: MouseEvent) => {
      if (e.target instanceof Node && !ref.current?.contains(e.target)) closeRef.current(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeRef.current(true);
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
  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) ref.current?.querySelector<HTMLElement>('[aria-expanded]')?.focus();
  };
  useDismiss(ref, open, close);
  return (
    <div
      className="popover-anchor"
      ref={ref}
      onBlur={(e) => {
        if (open && e.relatedTarget instanceof Node && !e.currentTarget.contains(e.relatedTarget)) close(false);
      }}
    >
      {trigger(open, () => setOpen((v) => !v))}
      {open && <div className={`popover popover-${align}${className ? ` ${className}` : ''}`}>{children(() => close(true))}</div>}
    </div>
  );
}

/** Arrow-key movement between the enabled items of a menu; the first item gets focus when it opens. */
const menuItems = (menu: HTMLElement | null) => [...(menu?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? [])];

function useMenuKeys() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    menuItems(ref.current)[0]?.focus();
  }, []);
  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const list = menuItems(ref.current);
    const at = list.indexOf(document.activeElement as HTMLElement);
    const move: Record<string, number> = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: list.length - 1 };
    const to = move[e.key];
    if (to === undefined || list.length === 0) return;
    e.preventDefault();
    list[(to + list.length) % list.length]?.focus();
  };
  return { ref, onKeyDown };
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
      {(close) => <MenuList label={label} items={items} close={close} />}
    </Popover>
  );
}

function MenuList({ label, items, close }: { label: string; items: MenuItem[]; close: () => void }) {
  const keys = useMenuKeys();
  return (
    <div role="menu" aria-label={label} className="menu" ref={keys.ref} onKeyDown={keys.onKeyDown}>
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
  );
}
