// Signed-in part of the app with Logto: finishes the sign-in callback, shows the sign-in page when signed out, and
// gives the API client the access token (refreshed by the SDK; one forced refresh after a 401).

import { useHandleSignInCallback, useLogto } from '@logto/react';
import { useQueryClient } from '@tanstack/react-query';
import { type ReactNode, useCallback, useEffect, useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

import { configureAuth } from '../../api/client';
import type { SignInConfig } from '../../api/tenancy';
import { CALLBACK_PATH, redirectUris } from './config';
import { MeGate } from './MeGate';
import { SignInPage } from './SignInPage';
import { Splash } from './Splash';

const RETURN_KEY = 'weft.return';

function rememberReturnPath(path: string): void {
  try {
    if (path !== CALLBACK_PATH && path !== '/') window.sessionStorage.setItem(RETURN_KEY, path);
  } catch {
    // Without session storage the user simply lands on the flows list.
  }
}

function takeReturnPath(): string {
  try {
    const path = window.sessionStorage.getItem(RETURN_KEY);
    window.sessionStorage.removeItem(RETURN_KEY);
    return path && path.startsWith('/') && !path.startsWith('//') ? path : '/flows';
  } catch {
    return '/flows';
  }
}

function Callback({ onRetry }: { onRetry: () => void }) {
  const navigate = useNavigate();
  const { isAuthenticated, error } = useHandleSignInCallback(() => navigate(takeReturnPath(), { replace: true }));
  // A reload of /callback after signing in has no code to exchange: just continue.
  useEffect(() => {
    if (isAuthenticated) navigate(takeReturnPath(), { replace: true });
  }, [isAuthenticated, navigate]);
  if (error) return <SignInPage onSignIn={onRetry} error={`Sign-in did not complete: ${error.message}`} />;
  return <Splash title="Signing you in…" />;
}

export function LogtoGate({ signIn: settings, children }: { signIn: SignInConfig; children: ReactNode }) {
  const { isAuthenticated, isLoading, signIn, signOut, getAccessToken, clearAccessToken, clearAllTokens } = useLogto();
  const qc = useQueryClient();
  const { pathname, search } = useLocation();
  const [opening, setOpening] = useState(false);
  const [problem, setProblem] = useState<string>();
  const uris = useMemo(
    () =>
      redirectUris(window.location.origin, {
        VITE_LOGTO_REDIRECT_URI: import.meta.env.VITE_LOGTO_REDIRECT_URI as string | undefined,
        VITE_LOGTO_POST_LOGOUT_REDIRECT_URI: import.meta.env.VITE_LOGTO_POST_LOGOUT_REDIRECT_URI as string | undefined,
      }),
    [],
  );
  const resource = settings.resource ?? undefined;

  const startSignIn = useCallback(() => {
    rememberReturnPath(`${pathname}${search}`);
    setOpening(true);
    setProblem(undefined);
    signIn(uris.signIn).catch((e: unknown) => {
      setOpening(false);
      setProblem(e instanceof Error ? e.message : String(e));
    });
  }, [pathname, search, signIn, uris.signIn]);

  // Signing out first takes the app off the screen, so nothing asks the API once the tokens are gone.
  const [leaving, setLeaving] = useState(false);
  const endSession = useCallback(() => setLeaving(true), []);
  useEffect(() => {
    if (!leaving) return;
    void qc.cancelQueries();
    configureAuth(null);
    qc.clear();
    signOut(uris.signOut).catch(() => signOut());
  }, [leaving, qc, signOut, uris.signOut]);

  // Registered while rendering so the first queries of the page already carry the token.
  if (isAuthenticated && !leaving) {
    configureAuth({
      token: async () => (await getAccessToken(resource)) ?? undefined,
      refresh: async () => {
        await clearAccessToken();
        return (await getAccessToken(resource)) ?? undefined;
      },
      onSignedOut: () => {
        configureAuth(null);
        void clearAllTokens().finally(() => window.location.assign('/'));
      },
    });
  }

  if (leaving) return <Splash title="Signing you out…" />;
  if (pathname === CALLBACK_PATH) return <Callback onRetry={startSignIn} />;
  if (!isAuthenticated) {
    if (isLoading && !opening) return <Splash title="Checking your sign-in…" />;
    return <SignInPage onSignIn={startSignIn} busy={opening} error={problem} />;
  }
  return (
    <MeGate signedIn signOut={endSession}>
      {children}
    </MeGate>
  );
}
