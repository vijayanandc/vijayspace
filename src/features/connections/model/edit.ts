// What an edit of a connection sends: the settings the person changed and the secrets they typed again.

import type { FieldSpec, JsonObject, JsonValue } from '../../../api/types';

/** The form's starting values: the stored settings; secret fields start empty (they are never sent back). */
export function editStart(fields: FieldSpec[], config: JsonObject): JsonObject {
  const out: JsonObject = {};
  for (const f of fields) {
    const v = config[f.key];
    if (!f.secret && v !== undefined) out[f.key] = v;
  }
  return out;
}

const same = (a: JsonValue | undefined, b: JsonValue | undefined) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

/** A secret the person typed: a non-empty text, or rows of secret headers. */
const typed = (v: JsonValue | undefined) => (typeof v === 'string' ? v !== '' : typeof v === 'object' && v !== null && Object.keys(v).length > 0);

/** Changed settings and non-empty secrets; `null` when there is nothing to send (a rename only). */
export function changedFields(fields: FieldSpec[], config: JsonObject, values: JsonObject): JsonObject | null {
  const out: JsonObject = {};
  for (const f of fields) {
    const v = values[f.key];
    if (f.secret) {
      if (typed(v) && v !== undefined) out[f.key] = v;
    } else if (!same(v, config[f.key])) {
      out[f.key] = v ?? null;
    }
  }
  return Object.keys(out).length > 0 ? out : null;
}

/** True when a setting (not a secret) differs from the stored one. */
export function settingChanged(fields: FieldSpec[], config: JsonObject, values: JsonObject, applies: (f: FieldSpec) => boolean): boolean {
  return fields.some((f) => !f.secret && applies(f) && !same(values[f.key], config[f.key]));
}

/**
 * A settings change needs the credentials again (the server refuses otherwise): the required secret fields of the
 * chosen sign-in that are still empty. Optional secrets (secret headers) not entered again are dropped.
 */
export function secretsStillNeeded(fields: FieldSpec[], config: JsonObject, values: JsonObject, applies: (f: FieldSpec) => boolean): string[] {
  if (!settingChanged(fields, config, values, applies)) return [];
  return fields.filter((f) => f.secret && f.required === true && applies(f) && !typed(values[f.key])).map((f) => f.label);
}
