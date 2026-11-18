// OAuth round trip helpers: where the provider sends the browser back to, and where the app goes next.

import { safeNextPath } from '../../lib/safePath';

export const OAUTH_RETURN_PATH = '/connections/callback';

/**
 * The `return_to` for `oauth/start`: the callback page, which finishes the sign-in in `org` (the org it was started
 * in) and then continues to `next` (e.g. the flow being edited).
 */
export function oauthReturnUrl(origin: string, next?: string, org?: string | null): string {
  const query = new URLSearchParams();
  const safe = safeNextPath(next ?? null);
  if (safe) query.set('next', safe);
  if (org) query.set('org', org);
  const q = query.toString();
  return `${origin}${OAUTH_RETURN_PATH}${q ? `?${q}` : ''}`;
}

const LOOPBACK = new Set(['127.0.0.1', '[::1]', 'localhost']);

/**
 * Whether the browser may be sent to `url` to sign in to an app: an https page, this app itself, or a plain-http
 * page on this machine (a local test double of the app, e.g. the backend's `scripts/zoho_mock.py`). The URL comes
 * from the server; checking it keeps a `javascript:` or plain-http address elsewhere from ever being followed.
 */
export function signInUrlAllowed(url: string, here: string = window.location.href): boolean {
  if (!url.trim()) return false;
  try {
    const target = new URL(url, here);
    if (target.protocol === 'https:' || target.origin === new URL(here).origin) return true;
    return target.protocol === 'http:' && LOOPBACK.has(target.hostname);
  } catch {
    return false;
  }
}

/** Leaves for the app's sign-in page (see `signInUrlAllowed`). */
export function leaveForSignIn(url: string): void {
  if (!signInUrlAllowed(url)) throw new Error('The app sent an unsafe sign-in address, so sign-in was not started.');
  window.location.assign(url);
}
