// A mapping input for values that are not plain text (numbers, JSON): it keeps exactly what was typed
// and stores the parsed value, so "1." or "[1, " survive while typing instead of being re-rendered.

import { useState } from 'react';

import type { JsonValue } from '../../../api/types';
import { sameJson, valueText } from '../model/values';
import { MappingInput, type MappingInputProps } from './MappingInput';

interface Props extends Omit<MappingInputProps, 'value' | 'onChange'> {
  value: JsonValue | undefined;
  onChange: (value: JsonValue) => void;
  /** Typed text → value to store, or undefined to store nothing yet (an incomplete number). */
  parse: (text: string) => JsonValue | undefined;
  /** How a stored value is shown (defaults to the text, or JSON for non-strings). */
  format?: (value: JsonValue | undefined) => string;
}

export function ParsedMappingInput({ value, onChange, parse, format = valueText, ...rest }: Props) {
  const [text, setText] = useState(() => format(value));
  const [seen, setSeen] = useState(value);
  if (!sameJson(seen, value)) {
    // An outside change (undo, a picked variable): show it unless the typed text already means it.
    setSeen(value);
    const typed = parse(text);
    if (typed === undefined || !sameJson(typed, value)) setText(format(value));
  }
  return (
    <MappingInput
      {...rest}
      value={text}
      onChange={(next) => {
        setText(next);
        const parsed = parse(next);
        if (parsed !== undefined) onChange(parsed);
      }}
    />
  );
}
