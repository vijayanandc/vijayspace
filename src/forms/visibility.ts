// Which fields of a form apply: a field with `show_when` is shown only while another field holds one of the listed
// values (e.g. the password of a connection only for "Username and password"). Mirrors the server's
// `FieldSpec::applies`: a field without a value counts as its default.

import type { FieldSpec, JsonObject } from '../api/types';

export function fieldApplies(field: FieldSpec, values: JsonObject, all: FieldSpec[]): boolean {
  const rule = field.show_when;
  if (!rule) return true;
  const own = values[rule.field];
  const current = own === undefined || own === null ? all.find((f) => f.key === rule.field)?.default : own;
  return typeof current === 'string' && rule.in.includes(current);
}

/** The fields that apply to `values`. */
export function visibleFields(fields: FieldSpec[], values: JsonObject): FieldSpec[] {
  return fields.filter((f) => fieldApplies(f, values, fields));
}
