// Logto client settings from the server's sign-in configuration, and the redirect URIs. The redirect URIs must be
// registered on the Logto app exactly (for local development: http://127.0.0.1:3000/callback).

import type { LogtoConfig } from '@logto/react';

import type { SignInConfig } from '../../api/tenancy';

export const CALLBACK_PATH = '/callback';

interface Env {
  VITE_LOGTO_REDIRECT_URI?: string;
  VITE_LOGTO_POST_LOGOUT_REDIRECT_URI?: string;
}

/** Where Logto sends the browser after signing in and out: build-time overrides, else this origin. */
export function redirectUris(origin: string, env: Env): { signIn: string; signOut: string } {
  const pick = (value: string | undefined, fallback: string) => (value && value.trim() ? value.trim() : fallback);
  return {
    signIn: pick(env.VITE_LOGTO_REDIRECT_URI, `${origin}${CALLBACK_PATH}`),
    signOut: pick(env.VITE_LOGTO_POST_LOGOUT_REDIRECT_URI, origin),
  };
}

/** The SDK configuration: the server's scopes plus `offline_access` (refresh tokens), and the API resource if any. */
export function logtoConfig(signIn: SignInConfig): LogtoConfig {
  const scopes = Array.from(new Set([...signIn.scopes, 'offline_access']));
  return {
    endpoint: signIn.endpoint,
    appId: signIn.app_id,
    scopes,
    ...(signIn.resource ? { resources: [signIn.resource] } : {}),
  };
}
