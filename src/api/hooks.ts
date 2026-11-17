// react-query hooks for shared, cacheable reads.

import { useQuery } from '@tanstack/react-query';

import { endpoints } from './endpoints';
import type { ConnectorManifest } from './types';

export const qk = {
  connectors: ['connectors'] as const,
  connections: ['connections'] as const,
  flows: ['flows'] as const,
  flow: (id: string) => ['flow', id] as const,
  executions: (filters: object) => ['executions', filters] as const,
  execution: (id: string) => ['execution', id] as const,
  recipes: ['recipes'] as const,
  org: ['org'] as const,
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

export function useConnections() {
  return useQuery({ queryKey: qk.connections, queryFn: endpoints.connections });
}

export function useOrg() {
  return useQuery({ queryKey: qk.org, queryFn: endpoints.org, staleTime: 10 * 60_000, retry: false });
}
