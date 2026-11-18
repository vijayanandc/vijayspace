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

/** Changed settings and non-empty secrets; `null` when there is nothing to send (a rename only). */
export function changedFields(fields: FieldSpec[], config: JsonObject, values: JsonObject): JsonObject | null {
  const out: JsonObject = {};
  for (const f of fields) {
    const v = values[f.key];
    if (f.secret) {
      if (typeof v === 'string' && v !== '') out[f.key] = v;
    } else if (!same(v, config[f.key])) {
      out[f.key] = v ?? null;
    }
  }
  return Object.keys(out).length > 0 ? out : null;
}

/** A settings change needs every secret again (the server refuses otherwise): the secret fields still empty. */
export function secretsStillNeeded(fields: FieldSpec[], config: JsonObject, values: JsonObject, applies: (f: FieldSpec) => boolean): string[] {
  const settingChanged = fields.some((f) => !f.secret && applies(f) && !same(values[f.key], config[f.key]));
  if (!settingChanged) return [];
  return fields.filter((f) => f.secret && applies(f) && !(typeof values[f.key] === 'string' && values[f.key] !== '')).map((f) => f.label);
}
