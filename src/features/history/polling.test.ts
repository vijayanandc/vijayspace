import { describe, expect, it } from 'vitest';

import { executionPollMs, historyPollMs } from './polling';

describe('history polling', () => {
  const now = Date.parse('2026-11-18T10:00:00Z');

  it('stops for finished runs and for runs that cannot be loaded', () => {
    expect(executionPollMs('succeeded', false, null, now)).toBe(false);
    expect(executionPollMs(undefined, true, null, now)).toBe(false);
    expect(executionPollMs('running', false, null, now)).toBe(2000);
    expect(executionPollMs(undefined, false, null, now)).toBe(2000);
  });

  it('looks at a waiting run when its delay ends, and at most every minute', () => {
    expect(executionPollMs('waiting', false, '2026-11-18T10:00:10Z', now)).toBe(10_000);
    expect(executionPollMs('waiting', false, '2026-11-19T10:00:00Z', now)).toBe(60_000);
    expect(executionPollMs('waiting', false, null, now)).toBe(30_000);
  });

  it('refreshes the list only while one page is shown', () => {
    expect(historyPollMs(1)).toBe(5000);
    expect(historyPollMs(3)).toBe(false);
  });
});
