// The session: who is signed in, the org they are acting in (sent as X-Org-ID), switching orgs and signing out.
// With sign-in turned off on the server, the session is a local owner of the default org.

import { useQueryClient } from '@tanstack/react-query';
import { type ReactNode, createContext, useCallback, useContext, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

import { setCurrentOrg } from '../../api/client';
import { qk } from '../../api/hooks';
import type { Me, Membership, Role } from '../../api/tenancy';
import { read, write } from '../../lib/storage';

const ORG_KEY = 'weft.org';

export interface Session {
  me: Me;
  org: Membership;
  /** False when sign-in is turned off on the server (local development). */
  signedIn: boolean;
  switchOrg: (orgId: string) => void;
  signOut: () => void;
}

const SessionContext = createContext<Session | null>(null);

/** The current session, or null outside one (tests of single components, sign-in turned off before /me). */
export function useSession(): Session | null {
  return useContext(SessionContext);
}

/** The caller's role in the current org; owner when there is no session (sign-in turned off). */
export function useRole(): Role {
  return useSession()?.org.role ?? 'owner';
}

/** The signed-in user's ID (null with sign-in turned off). */
export function useUserId(): string | null {
  return useSession()?.me.user?.id ?? null;
}

/** The org to act in: the remembered one if the user still belongs to it, else their first. */
export function pickOrg(orgs: Membership[], stored: string | null): Membership | undefined {
  return orgs.find((o) => o.id === stored) ?? orgs[0];
}

interface Props {
  me: Me;
  signedIn: boolean;
  signOut: () => void;
  children: ReactNode;
}

export function SessionProvider({ me, signedIn, signOut, children }: Props) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const org = pickOrg(me.orgs, read(ORG_KEY));
  // Set while rendering (an idempotent module variable), so the children's first queries already carry the org.
  setCurrentOrg(org?.id ?? null);
  const switchOrg = useCallback(
    (orgId: string) => {
      write(ORG_KEY, orgId);
      setCurrentOrg(orgId);
      // Nothing from the previous org may show in the next one.
      queryClient.removeQueries({ predicate: (q) => q.queryKey[0] !== qk.me[0] && q.queryKey[0] !== qk.authConfig[0] });
      void queryClient.invalidateQueries({ queryKey: qk.me });
      navigate('/flows');
    },
    [queryClient, navigate],
  );
  const value = useMemo(
    () => (org ? { me, org, signedIn, switchOrg, signOut } : null),
    [me, org, signedIn, switchOrg, signOut],
  );
  if (!value) return null;
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

/** For tests: a session with one org and the given role. */
export function TestSession({ role, children, userId = 'usr_me' }: { role: Role; children: ReactNode; userId?: string }) {
  const org: Membership = {
    id: 'org_test', name: 'Test org', timezone: 'UTC', retention_days: null, retention_days_effective: 30, retention_days_default: 30, role,
  };
  const me: Me = { user: { id: userId, email: 'me@example.com', email_verified: true, name: 'Me', created_at: '' }, orgs: [org], invitations: [] };
  const value: Session = { me, org, signedIn: true, switchOrg: () => undefined, signOut: () => undefined };
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}
