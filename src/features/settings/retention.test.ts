import { describe, expect, it } from 'vitest';

import { retentionBody, retentionChanged, retentionError, retentionForm, retentionNote, retentionShrink } from './retention';

describe('retention form', () => {
  it('starts on the server default when the org has no setting', () => {
    expect(retentionForm({ retention_days: null, retention_days_effective: 30 })).toEqual({ useDefault: true, days: '30' });
    expect(retentionForm({ retention_days: 14, retention_days_effective: 14 })).toEqual({ useDefault: false, days: '14' });
    // Saved before the 30-day cap: shown (and saved again) as the cap, so the page does not open with an error.
    expect(retentionForm({ retention_days: 90, retention_days_effective: 30 })).toEqual({ useDefault: false, days: '30' });
  });

  it('sends null for the server default and the number of days otherwise', () => {
    expect(retentionBody({ useDefault: true, days: 'junk' })).toEqual({ retention_days: null });
    expect(retentionBody({ useDefault: false, days: ' 14 ' })).toEqual({ retention_days: 14 });
  });

  it('accepts whole days from 1 to 30 (the platform cap)', () => {
    expect(retentionError({ useDefault: false, days: '1' })).toBeNull();
    expect(retentionError({ useDefault: false, days: '30' })).toBeNull();
    expect(retentionError({ useDefault: false, days: '0' })).toMatch(/1 to 30 days/);
    expect(retentionError({ useDefault: false, days: '31' })).toMatch(/1 to 30 days/);
    expect(retentionError({ useDefault: false, days: '7.5' })).toMatch(/whole number/);
    expect(retentionError({ useDefault: false, days: '' })).toMatch(/Enter/);
    expect(retentionBody({ useDefault: false, days: '0' })).toBeNull();
  });

  it('knows whether saving changes anything', () => {
    expect(retentionChanged({ useDefault: true, days: '30' }, { retention_days: null })).toBe(false);
    expect(retentionChanged({ useDefault: false, days: '30' }, { retention_days: null })).toBe(true);
    expect(retentionChanged({ useDefault: false, days: '14' }, { retention_days: 14 })).toBe(false);
  });

  it('describes the retention', () => {
    expect(retentionNote(30)).toBe('Runs are kept for 30 days');
    expect(retentionNote(1)).toBe('Runs are kept for 1 day');
  });

  it('knows when saving would delete runs sooner', () => {
    const org = { retention_days_effective: 30, retention_days_default: 30 };
    expect(retentionShrink({ useDefault: false, days: '7' }, org)).toEqual({ from: 30, to: 7 });
    expect(retentionShrink({ useDefault: false, days: '30' }, org)).toBeNull();
    expect(retentionShrink({ useDefault: true, days: '' }, { retention_days_effective: 30, retention_days_default: 14 })).toEqual({ from: 30, to: 14 });
    expect(retentionShrink({ useDefault: false, days: 'x' }, org)).toBeNull();
  });
});
