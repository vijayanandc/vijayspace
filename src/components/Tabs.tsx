// Fluent-style tab list with a brand indicator under the selected tab.

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
  return (
    <div className={`tabs${className ? ` ${className}` : ''}`} role="tablist" aria-label={label}>
      {tabs.map((t) => (
        <button
          key={t.key}
          type="button"
          role="tab"
          aria-selected={t.key === value}
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
