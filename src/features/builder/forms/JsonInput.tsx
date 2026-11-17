// JSON text editor: keeps the text while it does not parse (or fails `validate`); emits parsed values.

import { useEffect, useState } from 'react';

import type { JsonValue } from '../../../api/types';

interface Props {
  id: string;
  value: JsonValue | undefined;
  onChange: (v: JsonValue) => void;
  rows?: number;
  /** Shape check for parsed JSON: a message keeps the text and blocks `onChange`. */
  validate?: (v: JsonValue) => string | null;
}

export function JsonInput({ id, value, onChange, rows = 6, validate }: Props) {
  const serialized = value === undefined ? '' : JSON.stringify(value, null, 2);
  const [text, setText] = useState(serialized);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    // Accept outside changes (undo, test samples) when the text is not mid-edit.
    if (!error) setText(serialized);
  }, [serialized, error]);
  const accept = (v: JsonValue) => {
    const problem = validate?.(v) ?? null;
    setError(problem);
    if (!problem) onChange(v);
  };
  return (
    <div>
      <textarea
        id={id}
        className="input code"
        rows={rows}
        spellCheck={false}
        value={text}
        aria-invalid={error ? true : undefined}
        onChange={(e) => {
          setText(e.target.value);
          if (e.target.value.trim() === '') return accept(null);
          let parsed: JsonValue;
          try {
            parsed = JSON.parse(e.target.value) as JsonValue;
          } catch (err) {
            setError(`Invalid JSON: ${err instanceof Error ? err.message : 'it does not parse'}`);
            return undefined;
          }
          return accept(parsed);
        }}
      />
      {error && <div className="field-error-text">{error}</div>}
    </div>
  );
}
