// The app's query cache, partitioned by organisation: every query key is hashed together with the current org
// (X-Org-ID), so switching orgs can never show data cached for another one. The signed-in user and the sign-in
// configuration belong to no org.

import { QueryClient, type QueryKey, hashKey } from '@tanstack/react-query';

import { getCurrentOrg } from './client';

const ORG_FREE = new Set(['me', 'auth-config']);

export function orgScopedHash(key: QueryKey): string {
  return hashKey(ORG_FREE.has(String(key[0])) ? key : [getCurrentOrg(), ...key]);
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: 1, refetchOnWindowFocus: false, queryKeyHashFn: orgScopedHash } },
  });
}
