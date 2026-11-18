// Typed text → stored values: integer settings (clamped to the server's integer types), delay units,
// mappable number fields and loosely-typed values (set variable).

import type { JsonValue } from '../api/types';

export interface IntRange {
  min: number;
  max: number;
}

export const U16_MAX = 65_535;
export const U32_MAX = 4_294_967_295;
/** The server's longest "every N minutes" interval (one year). */
export const MAX_INTERVAL_MINUTES = 525_600;
/** The server rejects delays longer than this (`MAX_DELAY_SECS`). */
export const MAX_DELAY_SECS = 30 * 86_400;

/** Rounds to an integer and clamps it into `range`. */
export function clampInt(n: number, { min, max }: IntRange): number {
  return Math.min(max, Math.max(min, Math.round(n)));
}

/** The integer typed into a numeric setting, rounded and clamped; undefined while the text is empty or not a number. */
export function parseInteger(text: string, range: IntRange): number | undefined {
  const t = text.trim();
  if (t === '') return undefined;
  const n = Number(t);
  return Number.isFinite(n) ? clampInt(n, range) : undefined;
}

/** Delay units offered in the panel, in seconds (`1` only appears for durations that are not whole minutes). */
export const DELAY_UNITS = [60, 3600, 86_400] as const;

/** The largest unit that divides `seconds` exactly (so the amount shows as a whole number). */
export function bestDelayUnit(seconds: number): number {
  return [...DELAY_UNITS].reverse().find((u) => seconds % u === 0) ?? 1;
}

/** The unit to show: the one the user chose while it still divides `seconds`, else the best fit. */
export function delayUnit(seconds: number, chosen: number | null): number {
  return chosen !== null && seconds % chosen === 0 ? chosen : bestDelayUnit(seconds);
}

/** Whole-number amount of `unit`s → seconds (at least one unit, at most the server's maximum delay). */
export function delaySeconds(amount: number, unit: number): number {
  return clampInt(amount, { min: 1, max: Math.max(1, Math.floor(MAX_DELAY_SECS / unit)) }) * unit;
}

/** Keeps the shown amount and switches the unit (e.g. 31 minutes → 31 hours). */
export function changeDelayUnit(seconds: number, from: number, to: number): number {
  return delaySeconds(seconds / from, to);
}

const COMPLETE_NUMBER = /^-?(\d+(\.\d+)?|\.\d+)([eE][-+]?\d+)?$/;
const PARTIAL_NUMBER = /^-?\d*\.?\d*([eE][-+]?\d*)?$/;

/**
 * Text typed into a mappable number field → the value to store: a number once the text is a complete
 * number, the text itself for templates and anything else, and undefined while a number is still being
 * typed ("1.", "-", "2e"), so the field keeps the raw text instead of re-rendering it.
 */
export function parseNumberField(text: string): JsonValue | undefined {
  const t = text.trim();
  if (t === '' || t.includes('{{')) return text;
  if (COMPLETE_NUMBER.test(t)) return Number(t);
  if (PARTIAL_NUMBER.test(t)) return undefined;
  return text;
}

/**
 * Text typed as a set-variable value → JSON when it is JSON other than a string (arrays, objects, numbers,
 * true/false/null), otherwise the text. Numbers are only converted when they survive the round trip, so
 * long record IDs and "1.50" stay text instead of losing digits.
 */
export function parseLooseValue(text: string): JsonValue {
  const t = text.trim();
  if (t === '') return text;
  try {
    const value = JSON.parse(t) as JsonValue;
    if (typeof value === 'string') return text;
    if (typeof value === 'number' && String(value) !== t) return text;
    return value;
  } catch {
    return text;
  }
}

/** How a stored value is shown in a text box. */
export function valueText(value: JsonValue | undefined): string {
  if (value === undefined || value === null) return '';
  return typeof value === 'string' ? value : JSON.stringify(value);
}

/** Value equality for JSON values (key order included, which is what the editor produces). */
export function sameJson(a: JsonValue | undefined, b: JsonValue | undefined): boolean {
  return a === b || JSON.stringify(a) === JSON.stringify(b);
}
