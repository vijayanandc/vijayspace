// What dynamic form fields need to know about where they are rendered.

import type { Issue } from '../api/types';
import type { VarNode } from '../domain/variables';

export interface FormEnv {
  connector: string;
  connectionId?: string | null;
  /** When true, dynamic options/fields wait for a connection to be chosen. */
  requiresConnection: boolean;
  variables: VarNode[];
  /** Validation issues of the step being edited (full paths such as `/steps/0/input/code`). */
  issues?: Issue[];
}

/** Splits text into plain and `{{ … }}` token segments (for highlighting). */
export function tokenize(text: string): { text: string; token: boolean }[] {
  const out: { text: string; token: boolean }[] = [];
  const re = /\{\{.*?\}\}/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    const start = m.index ?? 0;
    if (start > last) out.push({ text: text.slice(last, start), token: false });
    out.push({ text: m[0], token: true });
    last = start + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last), token: false });
  return out;
}

/** Inserts `{{ path }}` into `value` at the cursor range. */
export function insertToken(value: string, path: string, start: number, end: number): { value: string; cursor: number } {
  const token = `{{ ${path} }}`;
  const s = Math.max(0, Math.min(start, value.length));
  const e = Math.max(s, Math.min(end, value.length));
  return { value: value.slice(0, s) + token + value.slice(e), cursor: s + token.length };
}
