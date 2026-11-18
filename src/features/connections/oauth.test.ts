import { describe, expect, it } from 'vitest';

import { oauthReturnUrl, signInUrlAllowed } from './oauth';

describe('OAuth return paths', () => {
  it('returns to the callback page, carrying the in-app page to continue to', () => {
    expect(oauthReturnUrl('http://localhost:5173')).toBe('http://localhost:5173/connections/callback');
    expect(oauthReturnUrl('http://localhost:5173', '/flows/flw_1')).toBe('http://localhost:5173/connections/callback?next=%2Fflows%2Fflw_1');
    expect(oauthReturnUrl('http://localhost:5173', undefined, 'org_a')).toBe('http://localhost:5173/connections/callback?org=org_a');
  });

  it('follows only https sign-in pages (or this app)', () => {
    const here = 'http://127.0.0.1:3000/connections';
    expect(signInUrlAllowed('https://accounts.zoho.com/oauth/v2/auth?x=1', here)).toBe(true);
    expect(signInUrlAllowed('#signin', here)).toBe(true);
    for (const bad of ['javascript:alert(1)', 'http://evil.example/auth', 'data:text/html,hi', '']) {
      expect(signInUrlAllowed(bad, 'https://app.weft.test/x')).toBe(false);
    }
  });
});
