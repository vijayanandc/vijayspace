// Numbered, monospaced log lines (custom function `print` output) under a small header.

import { ScrollText } from 'lucide-react';

export function LogBlock({ lines, label = 'Log' }: { lines: string[]; label?: string }) {
  if (lines.length === 0) return null;
  return (
    <section className="log-block" aria-label={label}>
      <div className="log-head">
        <ScrollText size={14} aria-hidden="true" />
        <span className="log-title">{label}</span>
        <span className="log-count">
          {lines.length} line{lines.length === 1 ? '' : 's'}
        </span>
      </div>
      <ol className="log-lines">
        {lines.map((line, i) => (
          // Lines can repeat; their position is their identity.
          <li key={i}>{line}</li>
        ))}
      </ol>
    </section>
  );
}
