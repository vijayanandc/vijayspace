// Whole-number input for settings the server stores as integers (seconds, attempts, minutes).

import { useState } from 'react';

import { type IntRange, parseInteger } from '../domain/values';

interface Props {
  id?: string;
  value: number | undefined;
  /** Receives the rounded, clamped integer; `undefined` when an optional field is cleared. */
  onChange: (n: number | undefined) => void;
  min: number;
  max: number;
  /** When true, clearing the field emits `undefined` (the server default applies). */
  optional?: boolean;
  placeholder?: string;
  className?: string;
}

const show = (n: number | undefined) => (n === undefined ? '' : String(n));

/** Keeps the typed text while editing (so "" or "3" on the way to "300" is not snapped), and emits only valid integers. */
export function IntInput({ id, value, onChange, min, max, optional = false, placeholder, className = 'input' }: Props) {
  const range: IntRange = { min, max };
  const [text, setText] = useState(show(value));
  const [seen, setSeen] = useState(value);
  if (value !== seen) {
    // An outside change (undo, another node): show it unless it is what the text already means.
    setSeen(value);
    if (parseInteger(text, range) !== value) setText(show(value));
  }
  return (
    <input
      id={id}
      className={className}
      type="number"
      inputMode="numeric"
      step={1}
      min={min}
      max={max}
      placeholder={placeholder}
      value={text}
      onChange={(e) => {
        setText(e.target.value);
        const n = parseInteger(e.target.value, range);
        if (n !== undefined) onChange(n);
        else if (optional && e.target.value.trim() === '') onChange(undefined);
      }}
      onBlur={() => setText(show(value))}
    />
  );
}
