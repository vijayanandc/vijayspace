// Decides how the app signs in: the server's sign-in configuration turns Logto on (or leaves sign-in off for local
// development), then the signed-in session wraps the routed app.

import { LogtoProvider } from '@logto/react';
import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useMemo } from 'react';

import { configureAuth, errorMessage } from '../../api/client';
import { qk } from '../../api/hooks';
import { type SignInConfig, tenancy } from '../../api/tenancy';
import { Button } from '../../components/Button';
import { logtoConfig } from './config';
import { LogtoGate } from './LogtoGate';
import { MeGate } from './MeGate';
import { Splash } from './Splash';
import { ensureSubtleDigest } from './webCrypto';

const noop = () => undefined;

function WithLogto({ signIn, children }: { signIn: SignInConfig; children: ReactNode }) {
  const config = useMemo(() => {
    // Plain-HTTP origins other than localhost have no crypto.subtle; the SDK needs SHA-256 for PKCE.
    ensureSubtleDigest();
    return logtoConfig(signIn);
  }, [signIn]);
  return (
    <LogtoProvider config={config}>
      <LogtoGate signIn={signIn}>{children}</LogtoGate>
    </LogtoProvider>
  );
}

export function AuthRoot({ children }: { children: ReactNode }) {
  const config = useQuery({ queryKey: qk.authConfig, queryFn: tenancy.authConfig, staleTime: Infinity, retry: 2 });
  if (config.isPending) return <Splash title="Loading…" />;
  if (config.isError) {
    return (
      <Splash title="Weft is not reachable">
        <p className="auth-error" role="alert">
          {errorMessage(config.error)}
        </p>
        <Button variant="primary" onClick={() => void config.refetch()}>
          Try again
        </Button>
      </Splash>
    );
  }
  const signIn = config.data.sign_in;
  if (!signIn) {
    configureAuth(null);
    return (
      <MeGate signedIn={false} signOut={noop}>
        {children}
      </MeGate>
    );
  }
  return <WithLogto signIn={signIn}>{children}</WithLogto>;
}
