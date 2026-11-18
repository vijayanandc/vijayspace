// Per-connection limits (pure): the form state of the "Limits & usage" dialog, its validation, the PUT body,
// and how usage is shown.

import type { ConnectionLimits, ConnectionUsage } from '../../api/types';

export const MAX_LIMIT = 10_000_000;

export type LimitKey = keyof ConnectionLimits;
export const LIMIT_KEYS: LimitKey[] = ['requests_per_minute', 'max_concurrent', 'daily_requests'];

export const LIMIT_LABELS: Record<LimitKey, { label: string; unit: string; help: string }> = {
  requests_per_minute: { label: 'Requests per minute', unit: '/ min', help: 'Spread evenly; short bursts up to the limit are allowed.' },
  max_concurrent: { label: 'Max concurrent requests', unit: 'at once', help: 'Requests in flight at the same time.' },
  daily_requests: { label: 'Requests per day', unit: '/ day', help: 'A budget per UTC day.' },
};

/** `default` sends null (the app's default), `none` sends 0 (no limit), `custom` sends the number. */
export type LimitMode = 'default' | 'none' | 'custom';
export interface LimitField {
  mode: LimitMode;
  /** The typed number (kept as text while editing). */
  value: string;
}
export type LimitsForm = Record<LimitKey, LimitField>;

/** The form for the connection's current overrides. */
export function limitsForm(overrides: ConnectionLimits): LimitsForm {
  const field = (n: number | null): LimitField => (n === null ? { mode: 'default', value: '' } : n === 0 ? { mode: 'none', value: '' } : { mode: 'custom', value: String(n) });
  return {
    requests_per_minute: field(overrides.requests_per_minute),
    max_concurrent: field(overrides.max_concurrent),
    daily_requests: field(overrides.daily_requests),
  };
}

/** A problem with one custom value, or null. */
export function limitError(field: LimitField): string | null {
  if (field.mode !== 'custom') return null;
  const text = field.value.trim().replace(/[,_\s]/g, '');
  if (text === '') return 'Enter a number, or choose the app default or no limit.';
  if (!/^\d+$/.test(text)) return 'Use a whole number.';
  const n = Number(text);
  if (n < 1) return 'Use 1 or more, or choose “No limit”.';
  if (n > MAX_LIMIT) return `Use at most ${MAX_LIMIT.toLocaleString('en-US')}.`;
  return null;
}

export function limitsErrors(form: LimitsForm): Partial<Record<LimitKey, string>> {
  const errors: Partial<Record<LimitKey, string>> = {};
  for (const key of LIMIT_KEYS) {
    const e = limitError(form[key]);
    if (e) errors[key] = e;
  }
  return errors;
}

/**
 * The PUT body: every key is sent, because the PUT replaces all overrides (null = the app default, 0 = no limit).
 * Returns null while a value is invalid.
 */
export function limitsBody(form: LimitsForm): ConnectionLimits | null {
  if (Object.keys(limitsErrors(form)).length > 0) return null;
  const value = (f: LimitField): number | null => (f.mode === 'default' ? null : f.mode === 'none' ? 0 : Number(f.value.trim().replace(/[,_\s]/g, '')));
  return {
    requests_per_minute: value(form.requests_per_minute),
    max_concurrent: value(form.max_concurrent),
    daily_requests: value(form.daily_requests),
  };
}

/** "10,000" or "No limit". */
export function formatLimit(n: number | null): string {
  return n === null || n === 0 ? 'No limit' : n.toLocaleString('en-US');
}

/** Width of the usage bar in percent (0–100), or null without a daily budget. */
export function usageBarPercent(u: Pick<ConnectionUsage, 'requests' | 'daily_limit' | 'percent'>): number | null {
  if (!u.daily_limit) return null;
  const pct = u.percent ?? (u.requests / u.daily_limit) * 100;
  return Math.max(0, Math.min(100, pct));
}

/** Spoken summary: "1,234 of 10,000 requests used today (12%), 3 refused" (or "1 request today"). */
export function usageSummary(u: Pick<ConnectionUsage, 'requests' | 'daily_limit' | 'refused' | 'percent'>): string {
  const used = u.requests.toLocaleString('en-US');
  const base = u.daily_limit
    ? `${used} of ${u.daily_limit.toLocaleString('en-US')} requests used today (${Math.floor(u.percent ?? (u.requests / u.daily_limit) * 100)}%)`
    : `${used} request${u.requests === 1 ? '' : 's'} today`;
  return u.refused > 0 ? `${base}, ${u.refused.toLocaleString('en-US')} refused` : base;
}

/** "1,234 / 10,000" (or "1,234 requests" without a budget). */
export function usageText(u: Pick<ConnectionUsage, 'requests' | 'daily_limit'>): string {
  const used = u.requests.toLocaleString('en-US');
  return u.daily_limit ? `${used} / ${u.daily_limit.toLocaleString('en-US')}` : `${used} request${u.requests === 1 ? '' : 's'}`;
}
