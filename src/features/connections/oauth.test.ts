import { describe, expect, it } from 'vitest';

import { oauthReturnUrl } from './oauth';

describe('OAuth return paths', () => {
  it('returns to the callback page, carrying the in-app page to continue to', () => {
    expect(oauthReturnUrl('http://localhost:5173')).toBe('http://localhost:5173/connections/callback');
    expect(oauthReturnUrl('http://localhost:5173', '/flows/flw_1')).toBe('http://localhost:5173/connections/callback?next=%2Fflows%2Fflw_1');
    expect(oauthReturnUrl('http://localhost:5173', undefined, 'org_a')).toBe('http://localhost:5173/connections/callback?org=org_a');
  });
});
