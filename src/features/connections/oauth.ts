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
