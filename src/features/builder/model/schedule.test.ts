import { describe, expect, it } from 'vitest';

import type { ScheduleSpec } from '../../../api/types';
import { detectPreset, presetToSpec, scheduleView } from './schedule';

describe('schedule presets', () => {
  it('detects daily, weekly and monthly cron shapes', () => {
    expect(detectPreset({ kind: 'cron', expr: '30 9 * * *', timezone: 'Asia/Kolkata' })).toMatchObject({ preset: 'daily', time: '09:30', timezone: 'Asia/Kolkata' });
    expect(detectPreset({ kind: 'cron', expr: '0 8 * * 1,3,5' })).toMatchObject({ preset: 'weekly', days: [1, 3, 5], time: '08:00' });
    expect(detectPreset({ kind: 'cron', expr: '15 6 1 * *' })).toMatchObject({ preset: 'monthly', dayOfMonth: 1, time: '06:15' });
    expect(detectPreset({ kind: 'cron', expr: '*/5 * * * *' })).toMatchObject({ preset: 'cron' });
    expect(detectPreset({ kind: 'interval', minutes: 10 })).toMatchObject({ preset: 'interval', minutes: 10 });
  });

  it('round-trips presets to specs', () => {
    for (const spec of [
      { kind: 'cron', expr: '30 9 * * *', timezone: 'UTC' },
      { kind: 'cron', expr: '0 8 * * 1,3,5', timezone: 'UTC' },
      { kind: 'cron', expr: '15 6 1 * *', timezone: 'UTC' },
      { kind: 'interval', minutes: 15 },
      { kind: 'once', at: '2026-12-01T09:00', timezone: 'UTC' },
    ] as const) {
      expect(presetToSpec(detectPreset(spec))).toEqual(spec);
    }
  });

  it('keeps the cron editor once chosen, with the expression exactly as typed', () => {
    const daily: ScheduleSpec = { kind: 'cron', expr: '0 9 * * *', timezone: 'UTC' };
    const chosen = scheduleView(daily, 'cron');
    expect(chosen).toMatchObject({ preset: 'cron', expr: '0 9 * * *' });
    expect(presetToSpec(chosen)).toEqual(daily);
    // Typing "0 9 * * 1-5" one key at a time never leaves the cron editor or loses a space.
    let spec: ScheduleSpec = { kind: 'cron', expr: '', timezone: 'UTC' };
    for (const ch of '0 9 * * 1-5') {
      const view = scheduleView(spec, 'cron');
      expect(view.preset).toBe('cron');
      spec = presetToSpec({ ...view, expr: view.expr + ch });
    }
    expect(spec).toEqual({ kind: 'cron', expr: '0 9 * * 1-5', timezone: 'UTC' });
    expect(scheduleView({ kind: 'cron', expr: '0 ' }, 'cron').expr).toBe('0 ');
  });

  it('detects the preset when the spec is not a cron the user is editing', () => {
    expect(scheduleView({ kind: 'cron', expr: '0 9 * * 1' }, 'daily').preset).toBe('weekly');
    expect(scheduleView({ kind: 'interval', minutes: 5 }, 'cron').preset).toBe('interval');
    expect(detectPreset({ kind: 'cron', expr: ' */5 * * * *' }).expr).toBe(' */5 * * * *');
  });
});
