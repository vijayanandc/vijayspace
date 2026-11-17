import { describe, expect, it } from 'vitest';

import { detectPreset, presetToSpec } from './schedule';

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
});
