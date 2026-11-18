import { afterEach, describe, expect, it } from 'vitest';

import { setCurrentOrg } from './client';
import { createQueryClient, orgScopedHash } from './queryClient';

describe('query cache per organisation', () => {
  afterEach(() => setCurrentOrg(null));

  it('keeps each org’s data apart, but not the signed-in user', () => {
    const client = createQueryClient();
    setCurrentOrg('org_a');
    client.setQueryData(['flows'], ['a-flow']);
    const meA = orgScopedHash(['me']);
    setCurrentOrg('org_b');
    expect(client.getQueryData(['flows'])).toBeUndefined();
    client.setQueryData(['flows'], ['b-flow']);
    expect(orgScopedHash(['me'])).toBe(meA);
    setCurrentOrg('org_a');
    expect(client.getQueryData(['flows'])).toEqual(['a-flow']);
  });
});
