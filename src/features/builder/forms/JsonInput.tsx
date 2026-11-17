import { useEffect, useState } from 'react';

import type { JsonValue } from '../../../api/types';

/** JSON editor: keeps the text while it does not parse; emits parsed values. */
export function JsonInput({ id, value, onChange, rows = 6 }: { id: string; value: JsonValue | undefined; onChange: (v: JsonValue) => void; rows?: number }) {
  const serialized = value === undefined ? '' : JSON.stringify(value, null, 2);
  const [text, setText] = useState(serialized);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    // Accept outside changes (undo, test samples) when the text is not mid-edit.
    if (!error) setText(serialized);
  }, [serialized, error]);
  return (
    <div>
      <textarea
        id={id}
        className="input code"
        rows={rows}
        spellCheck={false}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          if (e.target.value.trim() === '') {
            setError(null);
            onChange(null);
            return;
          }
          try {
            onChange(JSON.parse(e.target.value) as JsonValue);
            setError(null);
          } catch (err) {
            setError(err instanceof Error ? err.message : 'Invalid JSON');
          }
        }}
      />
      {error && <div className="field-error-text">Invalid JSON: {error}</div>}
    </div>
  );
}
