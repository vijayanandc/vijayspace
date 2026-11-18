// Signed-in part of the app with Logto: finishes the sign-in callback, shows the sign-in page when signed out, and
// gives the API client the access token (refreshed by the SDK; one forced refresh after a 401). A refresh Logto
// rejects ends the session; a network failure does not.

import { LogtoClientError, LogtoRequestError, useHandleSignInCallback, useLogto } from '@logto/react';
import { useQueryClient } from '@tanstack/react-query';
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

import { configureAuth } from '../../api/client';
import type { SignInConfig } from '../../api/tenancy';
import { CALLBACK_PATH, redirectUris } from './config';
import { MeGate } from './MeGate';
import { SignInPage } from './SignInPage';
import { Splash } from './Splash';

const RETURN_KEY = 'weft.return';
/** How long a navigation to Logto may take before the page offers the button again. */
const STUCK_MS = 8000;

function rememberReturnPath(pathname: string, search: string): void {
  try {
    if (pathname !== CALLBACK_PATH && pathname !== '/') window.sessionStorage.setItem(RETURN_KEY, `${pathname}${search}`);
  } catch {
    // Without session storage the user simply lands on the flows list.
  }
}

/** The page to open after signing in (read without removing it: React may run initializers twice). */
function peekReturnPath(): string {
  try {
    const path = window.sessionStorage.getItem(RETURN_KEY);
    return path && path.startsWith('/') && !path.startsWith('//') ? path : '/flows';
  } catch {
    return '/flows';
  }
}

function forgetReturnPath(): void {
  try {
    window.sessionStorage.removeItem(RETURN_KEY);
  } catch {
    // Nothing stored.
  }
}

/** Whether Logto says the session is over (as opposed to being unreachable). */
export function sessionEnded(error: unknown): boolean {
  if (error instanceof LogtoRequestError) return error.code === 'oidc.invalid_grant';
  if (error instanceof LogtoClientError) return error.code === 'not_authenticated';
  return false;
}

function Callback({ onRetry }: { onRetry: () => void }) {
  const navigate = useNavigate();
  const [target] = useState(peekReturnPath);
  const done = useRef(false);
  const finish = useCallback(() => {
    if (done.current) return;
    done.current = true;
    forgetReturnPath();
    navigate(target, { replace: true });
  }, [navigate, target]);
  const { isAuthenticated, isLoading, error } = useHandleSignInCallback(finish);
  const [waited, setWaited] = useState(false);
  useEffect(() => {
    if (isAuthenticated) finish();
  }, [isAuthenticated, finish]);
  // A /callback opened without a sign-in in progress (another tab, a bookmark) has nothing to finish.
  useEffect(() => {
    const timer = window.setTimeout(() => setWaited(true), 2500);
    return () => window.clearTimeout(timer);
  }, []);
  if (error) return <SignInPage onSignIn={onRetry} error={`Sign-in did not complete: ${error.message}`} />;
  if (waited && !isLoading && !isAuthenticated) return <SignInPage onSignIn={onRetry} />;
  return <Splash title="Signing you in…" />;
}

export function LogtoGate({ signIn: settings, children }: { signIn: SignInConfig; children: ReactNode }) {
  const { isAuthenticated, isLoading, error, signIn, signOut, getAccessToken, clearAccessToken, clearAllTokens } = useLogto();
  const qc = useQueryClient();
  const { pathname, search } = useLocation();
  const [opening, setOpening] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const signedOut = useRef(false);
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
    rememberReturnPath(pathname, search);
    setOpening(true);
    void signIn(uris.signIn);
  }, [pathname, search, signIn, uris.signIn]);

  // The SDK reports failures through `error` (its calls never reject): a failed start re-enables the button.
  useEffect(() => {
    if (!opening) return undefined;
    if (error) {
      setOpening(false);
      return undefined;
    }
    const timer = window.setTimeout(() => setOpening(false), STUCK_MS);
    return () => window.clearTimeout(timer);
  }, [opening, error]);

  const endLocally = useCallback(() => {
    signedOut.current = true;
    configureAuth(null);
    qc.clear();
    void clearAllTokens().finally(() => window.location.assign('/'));
  }, [qc, clearAllTokens]);

  // Logto rejected the refresh token (or there is none): the session is over, show the sign-in page.
  useEffect(() => {
    if (isAuthenticated && sessionEnded(error)) endLocally();
  }, [isAuthenticated, error, endLocally]);

  // Signing out first takes the app off the screen, so nothing asks the API once the tokens are gone. If Logto's
  // sign-out page cannot be reached, the tokens are cleared here instead.
  useEffect(() => {
    if (!leaving) return undefined;
    signedOut.current = true;
    void qc.cancelQueries();
    configureAuth(null);
    qc.clear();
    void signOut(uris.signOut);
    const timer = window.setTimeout(endLocally, STUCK_MS);
    return () => window.clearTimeout(timer);
  }, [leaving, qc, signOut, uris.signOut, endLocally]);

  // Registered while rendering so the first queries of the page already carry the token.
  if (isAuthenticated && !leaving && !signedOut.current) {
    configureAuth({
      token: async () => (await getAccessToken(resource)) ?? undefined,
      refresh: async () => {
        await clearAccessToken();
        return (await getAccessToken(resource)) ?? undefined;
      },
      onSignedOut: endLocally,
    });
  }

  if (leaving) return <Splash title="Signing you out…" />;
  if (pathname === CALLBACK_PATH) return <Callback onRetry={startSignIn} />;
  if (!isAuthenticated) {
    if (isLoading && !opening) return <Splash title="Checking your sign-in…" />;
    return <SignInPage onSignIn={startSignIn} busy={opening} error={error && !sessionEnded(error) ? error.message : undefined} />;
  }
  return (
    <MeGate signedIn signOut={() => setLeaving(true)}>
      {children}
    </MeGate>
  );
}
