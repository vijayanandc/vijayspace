import { describe, expect, it } from 'vitest';

import { safeNextPath } from './safePath';

describe('safe in-app paths', () => {
  it('only continues to in-app paths', () => {
    expect(safeNextPath('/flows/flw_1')).toBe('/flows/flw_1');
    for (const bad of [null, '', 'https://evil.example', '//evil.example', '/\\evil.example', '/\t/evil.example', '/\n/evil.example', '/\r/evil.example', '/ /evil.example', '/.//evil.example', '/..//evil.example', '/%2e//evil.example', '/flows/..//evil.example']) {
      expect(safeNextPath(bad, 'http://localhost:5173')).toBeNull();
    }
    expect(safeNextPath('/flows/flw_1?tab=x#y', 'http://localhost:5173')).toBe('/flows/flw_1?tab=x#y');
  });
});
