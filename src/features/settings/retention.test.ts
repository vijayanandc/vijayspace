import { describe, expect, it } from 'vitest';

import { retentionBody, retentionChanged, retentionError, retentionForm, retentionNote, retentionShrink } from './retention';

describe('retention form', () => {
  it('starts on the server default when the org has no setting', () => {
    expect(retentionForm({ retention_days: null, retention_days_effective: 30 })).toEqual({ useDefault: true, days: '30' });
    expect(retentionForm({ retention_days: 90, retention_days_effective: 90 })).toEqual({ useDefault: false, days: '90' });
  });

  it('sends null for the server default and the number of days otherwise', () => {
    expect(retentionBody({ useDefault: true, days: 'junk' })).toEqual({ retention_days: null });
    expect(retentionBody({ useDefault: false, days: ' 365 ' })).toEqual({ retention_days: 365 });
  });

  it('accepts whole days from 1 to 3650', () => {
    expect(retentionError({ useDefault: false, days: '1' })).toBeNull();
    expect(retentionError({ useDefault: false, days: '3650' })).toBeNull();
    expect(retentionError({ useDefault: false, days: '0' })).toMatch(/1 to 3,650/);
    expect(retentionError({ useDefault: false, days: '3651' })).toMatch(/1 to 3,650/);
    expect(retentionError({ useDefault: false, days: '7.5' })).toMatch(/whole number/);
    expect(retentionError({ useDefault: false, days: '' })).toMatch(/Enter/);
    expect(retentionBody({ useDefault: false, days: '0' })).toBeNull();
  });

  it('knows whether saving changes anything', () => {
    expect(retentionChanged({ useDefault: true, days: '30' }, { retention_days: null })).toBe(false);
    expect(retentionChanged({ useDefault: false, days: '30' }, { retention_days: null })).toBe(true);
    expect(retentionChanged({ useDefault: false, days: '90' }, { retention_days: 90 })).toBe(false);
  });

  it('describes the retention', () => {
    expect(retentionNote(30)).toBe('Runs are kept for 30 days');
    expect(retentionNote(1)).toBe('Runs are kept for 1 day');
  });

  it('knows when saving would delete runs sooner', () => {
    const org = { retention_days_effective: 30, retention_days_default: 30 };
    expect(retentionShrink({ useDefault: false, days: '7' }, org)).toEqual({ from: 30, to: 7 });
    expect(retentionShrink({ useDefault: false, days: '90' }, org)).toBeNull();
    expect(retentionShrink({ useDefault: true, days: '' }, { retention_days_effective: 90, retention_days_default: 30 })).toEqual({ from: 90, to: 30 });
    expect(retentionShrink({ useDefault: false, days: 'x' }, org)).toBeNull();
  });
});
