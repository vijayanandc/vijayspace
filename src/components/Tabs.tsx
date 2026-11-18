// Fluent-style tab list with a brand indicator under the selected tab. Keyboard: the selected tab is the tab stop;
// arrows, Home and End move to (and select) another tab.

import type { KeyboardEvent } from 'react';

export interface Tab<K extends string> {
  key: K;
  label: string;
  /** Optional count shown after the label. */
  count?: number;
}

interface Props<K extends string> {
  tabs: Tab<K>[];
  value: K;
  onChange: (k: K) => void;
  className?: string;
  /** Accessible name of the tab list. */
  label?: string;
}

export function Tabs<K extends string>({ tabs, value, onChange, className, label }: Props<K>) {
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const at = tabs.findIndex((t) => t.key === value);
    const to: Record<string, number> = { ArrowRight: at + 1, ArrowLeft: at - 1, Home: 0, End: tabs.length - 1 };
    const index = to[e.key];
    if (index === undefined || tabs.length === 0) return;
    e.preventDefault();
    const next = tabs[(index + tabs.length) % tabs.length];
    if (!next) return;
    onChange(next.key);
    const buttons = e.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]');
    buttons[(index + tabs.length) % tabs.length]?.focus();
  };
  return (
    <div className={`tabs${className ? ` ${className}` : ''}`} role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {tabs.map((t) => (
        <button
          key={t.key}
          type="button"
          role="tab"
          aria-selected={t.key === value}
          tabIndex={t.key === value ? 0 : -1}
          className={`tab${t.key === value ? ' tab-active' : ''}`}
          onClick={() => onChange(t.key)}
        >
          {t.label}
          {t.count !== undefined && <span className="tab-count">{t.count}</span>}
        </button>
      ))}
    </div>
  );
}
