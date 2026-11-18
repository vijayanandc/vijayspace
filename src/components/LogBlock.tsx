// Numbered, monospaced log lines (custom function `print` output) under a small header; long logs show their first
// lines until "Show all".

import { ScrollText } from 'lucide-react';
import { useState } from 'react';

/** Lines shown before "Show all". */
export const LOG_PREVIEW_LINES = 500;

export function LogBlock({ lines, label = 'Log' }: { lines: string[]; label?: string }) {
  const [all, setAll] = useState(false);
  if (lines.length === 0) return null;
  const shown = all ? lines : lines.slice(0, LOG_PREVIEW_LINES);
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
        {shown.map((line, i) => (
          // Lines can repeat; their position is their identity.
          <li key={i}>{line}</li>
        ))}
      </ol>
      {shown.length < lines.length && (
        <button type="button" className="link-btn log-more" onClick={() => setAll(true)}>
          Show all {lines.length.toLocaleString()} lines
        </button>
      )}
    </section>
  );
}
