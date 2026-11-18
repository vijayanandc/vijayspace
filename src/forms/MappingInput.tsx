// A text input (or textarea) that accepts `{{ variables }}` inserted from the picker.

import { Braces } from 'lucide-react';
import { useRef, useState } from 'react';

import type { VarNode } from '../domain/variables';
import { insertToken, tokenize } from './env';
import { VariablePicker } from './VariablePicker';

export interface MappingInputProps {
  id?: string;
  value: string;
  onChange: (value: string) => void;
  variables: VarNode[];
  multiline?: boolean;
  placeholder?: string;
  ariaLabel?: string;
  onBlur?: () => void;
}

export function MappingInput({ id, value, onChange, variables, multiline, placeholder, ariaLabel, onBlur }: MappingInputProps) {
  const [picking, setPicking] = useState(false);
  const ref = useRef<HTMLInputElement & HTMLTextAreaElement>(null);
  const pick = (path: string) => {
    const el = ref.current;
    const start = el?.selectionStart ?? value.length;
    const end = el?.selectionEnd ?? value.length;
    const next = insertToken(value, path, start, end);
    onChange(next.value);
    setPicking(false);
    window.requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(next.cursor, next.cursor);
    });
  };
  const hasTokens = value.includes('{{');
  return (
    <div className="mapping">
      <div className="mapping-row">
        {multiline ? (
          <textarea id={id} ref={ref} className="input mapping-input" rows={3} value={value} placeholder={placeholder} aria-label={ariaLabel} onChange={(e) => onChange(e.target.value)} onBlur={onBlur} />
        ) : (
          <input id={id} ref={ref} className="input mapping-input" value={value} placeholder={placeholder} aria-label={ariaLabel} onChange={(e) => onChange(e.target.value)} onBlur={onBlur} />
        )}
        <button type="button" className="map-btn" title="Insert variable" aria-label="Insert variable" onClick={() => setPicking(!picking)}>
          <Braces size={14} aria-hidden="true" />
        </button>
      </div>
      {hasTokens && (
        <div className="mapping-preview" aria-hidden="true">
          {tokenize(value).map((seg, i) => (seg.token ? <mark key={i} className="token">{seg.text}</mark> : <span key={i}>{seg.text}</span>))}
        </div>
      )}
      {picking && <VariablePicker tree={variables} onPick={pick} onClose={() => setPicking(false)} />}
    </div>
  );
}
