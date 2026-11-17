import { useState } from 'react';

import type { JsonValue } from '../api/types';

/** Collapsible JSON tree with copy. */
export function JsonView({ value, label, collapsed = false }: { value: JsonValue | undefined; label?: string; collapsed?: boolean }) {
  const [open, setOpen] = useState(!collapsed);
  if (value === undefined) return null;
  const text = JSON.stringify(value, null, 2);
  return (
    <div className="json-view">
      <div className="json-view-header">
        <button type="button" className="link-btn" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? '▾' : '▸'} {label ?? 'JSON'}
        </button>
        <button type="button" className="link-btn" onClick={() => void navigator.clipboard?.writeText(text)}>
          Copy
        </button>
      </div>
      {open && <pre className="json-view-body">{text}</pre>}
    </div>
  );
}
