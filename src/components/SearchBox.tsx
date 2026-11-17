// Search input with a leading magnifier icon (command bars, palette, global header).

import { Search } from 'lucide-react';
import type { InputHTMLAttributes } from 'react';

interface Props extends Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value'> {
  value: string;
  onChange: (value: string) => void;
  /** Accessible name. */
  label: string;
}

export function SearchBox({ value, onChange, label, className, ...rest }: Props) {
  return (
    <div className={`searchbox${className ? ` ${className}` : ''}`}>
      <Search size={16} className="searchbox-icon" aria-hidden="true" />
      <input {...rest} type="search" className="input searchbox-input" aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} />
    </div>
  );
}
