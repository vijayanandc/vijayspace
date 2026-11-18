// Run history retention (pure): the settings form state, its validation and the PATCH body.

import type { Org } from '../../api/types';

export const MIN_RETENTION_DAYS = 1;
export const MAX_RETENTION_DAYS = 3650;

export interface RetentionForm {
  /** True = use the server default (sends null). */
  useDefault: boolean;
  /** The typed number of days (kept as text while editing). */
  days: string;
}

export function retentionForm(org: Pick<Org, 'retention_days' | 'retention_days_effective'>): RetentionForm {
  return org.retention_days === null
    ? { useDefault: true, days: String(org.retention_days_effective) }
    : { useDefault: false, days: String(org.retention_days) };
}

export function retentionError(form: RetentionForm): string | null {
  if (form.useDefault) return null;
  const text = form.days.trim();
  if (text === '') return 'Enter a number of days.';
  if (!/^\d+$/.test(text)) return 'Use a whole number of days.';
  const n = Number(text);
  if (n < MIN_RETENTION_DAYS || n > MAX_RETENTION_DAYS) return `Use ${MIN_RETENTION_DAYS} to ${MAX_RETENTION_DAYS.toLocaleString('en-US')} days.`;
  return null;
}

/** The PATCH body, or null while the form is invalid. */
export function retentionBody(form: RetentionForm): { retention_days: number | null } | null {
  if (retentionError(form)) return null;
  return { retention_days: form.useDefault ? null : Number(form.days.trim()) };
}

/** True when saving would change the org's setting. */
export function retentionChanged(form: RetentionForm, org: Pick<Org, 'retention_days'>): boolean {
  const body = retentionBody(form);
  return body === null || body.retention_days !== org.retention_days;
}

/**
 * When saving would keep runs for fewer days than now (so older finished runs get deleted), the new and the
 * current number of days; otherwise null.
 */
export function retentionShrink(
  form: RetentionForm,
  org: Pick<Org, 'retention_days_effective' | 'retention_days_default'>,
): { from: number; to: number } | null {
  const body = retentionBody(form);
  if (!body) return null;
  const to = body.retention_days ?? org.retention_days_default;
  return to < org.retention_days_effective ? { from: org.retention_days_effective, to } : null;
}

/** "Runs are kept for 30 days" (for page meta lines). */
export function retentionNote(days: number): string {
  return `Runs are kept for ${days} day${days === 1 ? '' : 's'}`;
}
