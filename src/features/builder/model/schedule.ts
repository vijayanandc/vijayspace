// Friendly schedule presets ⇄ ScheduleSpec (cron under the hood).

import type { ScheduleSpec } from '../../../api/types';

export type Preset = 'interval' | 'daily' | 'weekly' | 'monthly' | 'once' | 'cron';

export interface PresetState {
  preset: Preset;
  time: string; // HH:MM
  days: number[]; // 0 = Sunday
  dayOfMonth: number;
  minutes: number;
  at: string;
  expr: string;
  timezone: string;
}

const pad = (n: number) => String(n).padStart(2, '0');

export function detectPreset(spec: ScheduleSpec): PresetState {
  const base: PresetState = { preset: 'daily', time: '09:00', days: [1], dayOfMonth: 1, minutes: 15, at: '', expr: '0 9 * * *', timezone: 'UTC' };
  if (spec.kind === 'interval') return { ...base, preset: 'interval', minutes: spec.minutes };
  if (spec.kind === 'once') return { ...base, preset: 'once', at: spec.at, timezone: spec.timezone ?? 'UTC' };
  const tz = spec.timezone ?? 'UTC';
  // Match on the trimmed text, but keep what the user typed (spaces included) for the cron editor.
  const expr = spec.expr.trim();
  const daily = /^(\d{1,2}) (\d{1,2}) \* \* \*$/.exec(expr);
  if (daily) return { ...base, preset: 'daily', time: `${pad(Number(daily[2]))}:${pad(Number(daily[1]))}`, expr, timezone: tz };
  const weekly = /^(\d{1,2}) (\d{1,2}) \* \* ([0-6](?:,[0-6])*)$/.exec(expr);
  if (weekly) {
    const days = (weekly[3] ?? '').split(',').map(Number);
    return { ...base, preset: 'weekly', time: `${pad(Number(weekly[2]))}:${pad(Number(weekly[1]))}`, days, expr, timezone: tz };
  }
  const monthly = /^(\d{1,2}) (\d{1,2}) (\d{1,2}) \* \*$/.exec(expr);
  if (monthly) {
    return { ...base, preset: 'monthly', time: `${pad(Number(monthly[2]))}:${pad(Number(monthly[1]))}`, dayOfMonth: Number(monthly[3]), expr, timezone: tz };
  }
  return { ...base, preset: 'cron', expr: spec.expr, timezone: tz };
}

/**
 * What the schedule panel shows for `spec` when the user last chose `mode`. The preset is detected only
 * when the panel opens (or the spec changes underneath it, e.g. undo); afterwards the chosen mode sticks, so
 * a cron expression that happens to look daily or weekly stays in the cron editor while it is typed.
 */
export function scheduleView(spec: ScheduleSpec, mode: Preset): PresetState {
  const detected = detectPreset(spec);
  if (spec.kind !== 'cron') return detected;
  if (mode === 'cron') return { ...detected, preset: 'cron', expr: spec.expr };
  return detected;
}

function hm(time: string): [number, number] {
  const [h, m] = time.split(':').map((x) => Number(x));
  return [Number.isFinite(h) ? (h ?? 0) : 0, Number.isFinite(m) ? (m ?? 0) : 0];
}

export function presetToSpec(s: PresetState): ScheduleSpec {
  const [h, m] = hm(s.time);
  switch (s.preset) {
    case 'interval':
      return { kind: 'interval', minutes: Math.max(1, Math.round(s.minutes)) };
    case 'once':
      return { kind: 'once', at: s.at, timezone: s.timezone };
    case 'daily':
      return { kind: 'cron', expr: `${m} ${h} * * *`, timezone: s.timezone };
    case 'weekly': {
      const days = [...new Set(s.days)].sort((a, b) => a - b);
      return { kind: 'cron', expr: `${m} ${h} * * ${days.length ? days.join(',') : '1'}`, timezone: s.timezone };
    }
    case 'monthly':
      return { kind: 'cron', expr: `${m} ${h} ${Math.min(31, Math.max(1, s.dayOfMonth))} * *`, timezone: s.timezone };
    case 'cron':
      return { kind: 'cron', expr: s.expr, timezone: s.timezone };
  }
}
