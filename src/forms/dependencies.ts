// When a dynamic dropdown or field list may load: its required dependencies are filled in, none is a template
// (`{{ … }}` resolves only when the flow runs), and typing in them has paused.

import { useEffect, useState } from 'react';

import type { FieldSpec, JsonObject } from '../api/types';

/** Whether `field`'s dependencies allow loading its options or fields. */
export function dependenciesReady(field: FieldSpec, siblings: JsonObject, siblingFields: FieldSpec[]): boolean {
  return (field.depends_on ?? []).every((key) => {
    const value = siblings[key];
    if (typeof value === 'string' && value.includes('{{')) return false;
    const spec = siblingFields.find((f) => f.key === key);
    return !spec?.required || (value !== undefined && value !== null && value !== '');
  });
}

/** A key for `field`'s dependency values that changes only once they stop changing for `ms`. */
export function useDependencyKey(field: FieldSpec, siblings: JsonObject, ms = 400): string {
  const key = JSON.stringify((field.depends_on ?? []).map((k) => siblings[k] ?? null));
  const [settled, setSettled] = useState(key);
  useEffect(() => {
    if (key === settled) return undefined;
    const timer = window.setTimeout(() => setSettled(key), ms);
    return () => window.clearTimeout(timer);
  }, [key, settled, ms]);
  return settled;
}
