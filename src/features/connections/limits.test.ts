import { describe, expect, it } from 'vitest';

import { formatLimit, limitError, limitsBody, limitsErrors, limitsForm, usageBarPercent, usageSummary, usageText } from './limits';

describe('limits form', () => {
  it('starts from the overrides: null = app default, 0 = no limit, n = custom', () => {
    expect(limitsForm({ requests_per_minute: 60, max_concurrent: null, daily_requests: 0 })).toEqual({
      requests_per_minute: { mode: 'custom', value: '60' },
      max_concurrent: { mode: 'default', value: '' },
      daily_requests: { mode: 'none', value: '' },
    });
  });

  it('builds the PUT body with every key (null = default, 0 = no limit)', () => {
    const form = limitsForm({ requests_per_minute: null, max_concurrent: null, daily_requests: null });
    form.requests_per_minute = { mode: 'custom', value: ' 1,200 ' };
    form.daily_requests = { mode: 'none', value: '' };
    expect(limitsBody(form)).toEqual({ requests_per_minute: 1200, max_concurrent: null, daily_requests: 0 });
  });

  it('round-trips the overrides unchanged', () => {
    const overrides = { requests_per_minute: 60, max_concurrent: 0, daily_requests: null };
    expect(limitsBody(limitsForm(overrides))).toEqual(overrides);
  });

  it('validates custom values: whole numbers from 1 to 10,000,000', () => {
    expect(limitError({ mode: 'custom', value: '' })).toMatch(/Enter a number/);
    expect(limitError({ mode: 'custom', value: '1.5' })).toMatch(/whole number/);
    expect(limitError({ mode: 'custom', value: '-3' })).toMatch(/whole number/);
    expect(limitError({ mode: 'custom', value: '0' })).toMatch(/No limit/);
    expect(limitError({ mode: 'custom', value: '10000001' })).toMatch(/at most 10,000,000/);
    expect(limitError({ mode: 'custom', value: '10000000' })).toBeNull();
    expect(limitError({ mode: 'default', value: 'junk' })).toBeNull();
    const form = limitsForm({ requests_per_minute: null, max_concurrent: null, daily_requests: null });
    form.max_concurrent = { mode: 'custom', value: 'x' };
    expect(Object.keys(limitsErrors(form))).toEqual(['max_concurrent']);
    expect(limitsBody(form)).toBeNull();
  });
});

describe('usage display', () => {
  it('formats limits and usage', () => {
    expect(formatLimit(null)).toBe('No limit');
    expect(formatLimit(0)).toBe('No limit');
    expect(formatLimit(10000)).toBe('10,000');
    expect(usageText({ requests: 1234, daily_limit: 10000 })).toBe('1,234 / 10,000');
    expect(usageText({ requests: 1, daily_limit: null })).toBe('1 request');
  });

  it('summarises usage for screen readers', () => {
    expect(usageSummary({ requests: 8500, daily_limit: 10000, percent: 85, refused: 3 })).toBe('8,500 of 10,000 requests used today (85%), 3 refused');
    expect(usageSummary({ requests: 1, daily_limit: null, percent: null, refused: 0 })).toBe('1 request today');
  });

  it('clamps the bar between 0 and 100 and hides it without a budget', () => {
    expect(usageBarPercent({ requests: 50, daily_limit: 100, percent: 50 })).toBe(50);
    expect(usageBarPercent({ requests: 150, daily_limit: 100, percent: 150 })).toBe(100);
    expect(usageBarPercent({ requests: 5, daily_limit: null, percent: null })).toBeNull();
  });
});
