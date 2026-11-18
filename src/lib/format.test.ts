import { describe, expect, it } from 'vitest';

import { formatDuration, inlineCodeParts, previewJson, updatedAgo } from './format';

describe('format', () => {
  it('describes how long ago a list was refreshed', () => {
    const now = 1_000_000_000;
    expect(updatedAgo(now - 5_000, now)).toBe('a few seconds ago');
    expect(updatedAgo(now - 60_000, now)).toBe('a minute ago');
    expect(updatedAgo(now - 10 * 60_000, now)).toBe('10 minutes ago');
    expect(updatedAgo(now - 3 * 3_600_000, now)).toBe('3 hours ago');
    expect(updatedAgo(now - 2 * 86_400_000, now)).toBe('2 days ago');
  });

  it('formats durations', () => {
    expect(formatDuration('2026-01-01T00:00:00Z', '2026-01-01T00:00:00.250Z')).toBe('250 ms');
    expect(formatDuration('2026-01-01T00:00:00Z', null)).toBe('—');
  });

  it('splits help text on backticks into code parts', () => {
    expect(inlineCodeParts('Read `input.<name>` here')).toEqual([
      { code: false, text: 'Read ' },
      { code: true, text: 'input.<name>' },
      { code: false, text: ' here' },
    ]);
    expect(inlineCodeParts('one ` alone')).toEqual([{ code: false, text: 'one ` alone' }]);
    expect(inlineCodeParts('plain')).toEqual([{ code: false, text: 'plain' }]);
  });
});

describe('previewJson', () => {
  it('matches JSON.stringify for small values and stops early for large ones', () => {
    expect(previewJson({ a: 1, b: [true, null, 'x'] })).toBe(JSON.stringify({ a: 1, b: [true, null, 'x'] }));
    expect(previewJson('plain text')).toBe('plain text');
    const big = { rows: Array.from({ length: 100_000 }, (_, i) => ({ id: i })) };
    expect(previewJson(big)).toBe(`${JSON.stringify(big).slice(0, 40)}…`);
  });
});
