// The session: who is signed in, the org they are acting in (sent as X-Org-ID), switching orgs and signing out.
// With sign-in turned off on the server, the session is a local owner of the default org.

import { type QueryClient, useQueryClient } from '@tanstack/react-query';
import { type ReactNode, createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { setCurrentOrg } from '../../api/client';
import { qk } from '../../api/hooks';
import type { Me, Membership, Role } from '../../api/tenancy';
import { read, write } from '../../lib/storage';
import { Splash } from './Splash';

const ORG_KEY = 'weft.org';

/**
 * The org this tab acts in: the tab's own choice (kept across the sign-in and OAuth round trips of this tab), else
 * the last choice in any tab (the default for a new tab).
 */
function rememberedOrg(orgs: Membership[]): string | null {
  const candidates = [read(ORG_KEY, 'session'), read(ORG_KEY)];
  return candidates.find((id) => id !== null && orgs.some((o) => o.id === id)) ?? null;
}

export function rememberOrg(orgId: string): void {
  write(ORG_KEY, orgId, 'session');
  write(ORG_KEY, orgId);
}

export interface Session {
  me: Me;
  org: Membership;
  /** False when sign-in is turned off on the server (local development). */
  signedIn: boolean;
  /** Acts in another org: the current page closes (pending saves finish first) and the flows list opens. */
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

/** The org to act in: the chosen one if the user still belongs to it, else their first. */
export function pickOrg(orgs: Membership[], chosen: string | null): Membership | undefined {
  return orgs.find((o) => o.id === chosen) ?? orgs[0];
}

/**
 * Resolves once nothing is being saved (a draft, org settings…; or after `maxMs`), a macrotask after being called.
 * Org switches and sign-out wait for it, so every save goes out in the org and sign-in it was made in, and nothing
 * it writes back into the cache lands in the next org.
 */
export function mutationsSettled(qc: QueryClient, maxMs = 10_000): Promise<void> {
  return new Promise((resolve) => {
    let unsubscribe = () => undefined as void;
    let finished = false;
    const giveUp = window.setTimeout(done, maxMs);
    function done() {
      if (finished) return;
      finished = true;
      window.clearTimeout(giveUp);
      unsubscribe();
      resolve();
    }
    const idle = () => qc.isMutating() === 0;
    // A save a page starts as it closes, or one queued behind a save that just finished, begins a few microtasks
    // later: an idle cache counts only once it is still idle a macrotask later.
    const check = () => {
      if (idle()) window.setTimeout(() => idle() && done(), 0);
    };
    window.setTimeout(() => {
      unsubscribe = qc.getMutationCache().subscribe(check);
      check();
    }, 0);
  });
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
  // Read once: another tab switching orgs must not change this one.
  const [chosen, setChosen] = useState(() => rememberedOrg(me.orgs));
  const [pending, setPending] = useState<string | null>(null);
  const org = pickOrg(me.orgs, chosen);
  // Set while rendering (an idempotent module variable), so the children's first queries already carry the org.
  if (!pending) setCurrentOrg(org?.id ?? null);

  const switchOrg = useCallback(
    (orgId: string) => {
      setPending(orgId);
      navigate('/flows');
    },
    [navigate],
  );

  // The page is off screen while switching; its last draft save (sent for the old org) finishes before the org
  // changes, and nothing cached for the old org is kept.
  useEffect(() => {
    if (!pending) return undefined;
    let cancelled = false;
    void mutationsSettled(queryClient).then(() => {
      if (cancelled) return;
      rememberOrg(pending);
      setCurrentOrg(pending);
      queryClient.removeQueries({ predicate: (q) => q.queryKey[0] !== qk.me[0] && q.queryKey[0] !== qk.authConfig[0] });
      setChosen(pending);
      setPending(null);
      void queryClient.invalidateQueries({ queryKey: qk.me });
    });
    return () => {
      cancelled = true;
    };
  }, [pending, queryClient]);

  const value = useMemo(
    () => (org ? { me, org, signedIn, switchOrg, signOut } : null),
    [me, org, signedIn, switchOrg, signOut],
  );
  if (!value) return null;
  return (
    <SessionContext.Provider value={value}>
      {pending ? <Splash title="Switching organisation…" /> : children}
    </SessionContext.Provider>
  );
}

/** For tests: a session with one org and the given role (`staff`: an owner or admin of the operator's default org). */
export function TestSession({ role, children, userId = 'usr_me', staff = false }: { role: Role; children: ReactNode; userId?: string; staff?: boolean }) {
  const org: Membership = {
    id: 'org_test', name: 'Test org', timezone: 'UTC', retention_days: null, retention_days_effective: 30, retention_days_default: 30, role,
  };
  const me: Me = { user: { id: userId, email: 'me@example.com', email_verified: true, name: 'Me', created_at: '' }, orgs: [org], invitations: [], staff, can_create_orgs: true };
  const value: Session = { me, org, signedIn: true, switchOrg: () => undefined, signOut: () => undefined };
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}
