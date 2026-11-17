import { describe, expect, it } from 'vitest';

import { oauthReturnUrl, safeNextPath } from './oauth';

describe('OAuth return paths', () => {
  it('returns to the callback page, carrying the in-app page to continue to', () => {
    expect(oauthReturnUrl('http://localhost:5173')).toBe('http://localhost:5173/connections/callback');
    expect(oauthReturnUrl('http://localhost:5173', '/flows/flw_1')).toBe('http://localhost:5173/connections/callback?next=%2Fflows%2Fflw_1');
  });

  it('only continues to in-app paths', () => {
    expect(safeNextPath('/flows/flw_1')).toBe('/flows/flw_1');
    for (const bad of [null, '', 'https://evil.example', '//evil.example', '/\\evil.example']) expect(safeNextPath(bad)).toBeNull();
  });
});
