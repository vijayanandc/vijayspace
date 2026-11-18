// react-query hooks for shared, cacheable reads.

import { useQuery } from '@tanstack/react-query';

import { endpoints } from './endpoints';
import type { ConnectorManifest } from './types';

export const qk = {
  connectors: ['connectors'] as const,
  connections: ['connections'] as const,
  /** Under `connections`, so invalidating the connections refreshes usage and limits too. */
  connectionUsage: ['connections', 'usage'] as const,
  connectionLimits: (id: string) => ['connections', id, 'limits'] as const,
  flows: ['flows'] as const,
  flow: (id: string) => ['flow', id] as const,
  executions: (filters: object) => ['executions', filters] as const,
  execution: (id: string) => ['execution', id] as const,
  /** Under `execution`, so refreshing a run refreshes its log too. */
  executionLog: (id: string) => ['execution', id, 'log'] as const,
  recipes: ['recipes'] as const,
  org: ['org'] as const,
  /** The signed-in user, their orgs and invitations (kept when switching orgs). */
  me: ['me'] as const,
  authConfig: ['auth-config'] as const,
  members: ['members'] as const,
  invitations: ['invitations'] as const,
};

/** Mutation keys, for mutations other components wait on (e.g. the builder waits for a draft save). */
export const mk = {
  saveDraft: (flowId: string) => ['save-draft', flowId] as const,
};

export function useConnectors() {
  return useQuery({ queryKey: qk.connectors, queryFn: endpoints.connectors, staleTime: 5 * 60_000 });
}

/** Manifest lookup by key (undefined while loading or unknown). */
export function useConnector(key: string | undefined): ConnectorManifest | undefined {
  const { data } = useConnectors();
  return key ? data?.find((c) => c.key === key) : undefined;
}

/** Lists change only through this app's own mutations (which invalidate them): reused for 30 s across pages. */
const LIST_STALE_MS = 30_000;

export function useConnections() {
  return useQuery({ queryKey: qk.connections, queryFn: endpoints.connections, staleTime: LIST_STALE_MS });
}

/** The org's flows (the flows page polls on top of this; search and filters reuse it). */
export function useFlows(options: { enabled?: boolean; refetchInterval?: number } = {}) {
  return useQuery({ queryKey: qk.flows, queryFn: endpoints.flows, staleTime: LIST_STALE_MS, ...options });
}

/** Today's usage of every connection (refreshed every 30 s while shown). */
export function useConnectionUsage() {
  return useQuery({ queryKey: qk.connectionUsage, queryFn: endpoints.connectionUsage, refetchInterval: 30_000, retry: false });
}

export function useOrg() {
  return useQuery({ queryKey: qk.org, queryFn: endpoints.org, staleTime: 10 * 60_000, retry: false });
}
