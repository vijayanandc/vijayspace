// Per-viewer UI preferences in localStorage (collapsed nav, list/grid view). Never required for correctness.

import { useCallback, useState } from 'react';

function read(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Storage can be unavailable (private mode, blocked site data); the preference is simply not kept.
  }
}

/** A string choice remembered across visits; values outside `allowed` fall back to `initial`. */
export function useStoredChoice<T extends string>(key: string, initial: T, allowed: readonly T[]): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(() => {
    const stored = read(key);
    return allowed.find((a) => a === stored) ?? initial;
  });
  const set = useCallback(
    (v: T) => {
      setValue(v);
      write(key, v);
    },
    [key],
  );
  return [value, set];
}
