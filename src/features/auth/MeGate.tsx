// Loads the signed-in user and their orgs, then shows onboarding (no org yet) or the app in a session.

import { useQuery } from '@tanstack/react-query';
import type { ReactNode } from 'react';

import { errorMessage } from '../../api/client';
import { qk } from '../../api/hooks';
import { tenancy } from '../../api/tenancy';
import { Button } from '../../components/Button';
import { Onboarding } from './Onboarding';
import { SessionProvider } from './session';
import { Splash } from './Splash';

export function MeGate({ signedIn, signOut, children }: { signedIn: boolean; signOut: () => void; children: ReactNode }) {
  const me = useQuery({ queryKey: qk.me, queryFn: tenancy.me, staleTime: 60_000 });
  if (me.isPending) return <Splash title="Loading your organisations…" />;
  if (me.isError) {
    return (
      <Splash title="Weft could not load your account">
        <p className="auth-error" role="alert">
          {errorMessage(me.error)}
        </p>
        <div className="form-actions">
          {signedIn && <Button onClick={signOut}>Sign out</Button>}
          <Button variant="primary" onClick={() => void me.refetch()}>
            Try again
          </Button>
        </div>
      </Splash>
    );
  }
  if (me.data.orgs.length === 0) return <Onboarding me={me.data} signOut={signOut} />;
  return (
    <SessionProvider me={me.data} signedIn={signedIn} signOut={signOut}>
      {children}
    </SessionProvider>
  );
}
