export interface Tab<K extends string> {
  key: K;
  label: string;
}

export function Tabs<K extends string>({ tabs, value, onChange }: { tabs: Tab<K>[]; value: K; onChange: (k: K) => void }) {
  return (
    <div className="tabs" role="tablist">
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
        </button>
      ))}
    </div>
  );
}
