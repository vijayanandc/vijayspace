// The only access to browser storage: per-viewer preferences in localStorage (collapsed nav, list/grid view, the
// current org) and per-tab values in sessionStorage (the page to return to after signing in). Never required for
// correctness: storage can be unavailable (private mode, blocked site data), and then nothing is kept.

import { useCallback, useState } from 'react';

/** `local`: kept across visits. `session`: this tab only. */
export type StorageArea = 'local' | 'session';

const area = (which: StorageArea): Storage => (which === 'local' ? window.localStorage : window.sessionStorage);

export function read(key: string, which: StorageArea = 'local'): string | null {
  try {
    return area(which).getItem(key);
  } catch {
    return null;
  }
}

export function write(key: string, value: string, which: StorageArea = 'local'): void {
  try {
    area(which).setItem(key, value);
  } catch {
    // Not kept.
  }
}

export function remove(key: string, which: StorageArea = 'local'): void {
  try {
    area(which).removeItem(key);
  } catch {
    // Nothing stored.
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
