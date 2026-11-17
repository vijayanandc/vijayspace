// OAuth round trip helpers: where the provider sends the browser back to, and where the app goes next.

export const OAUTH_RETURN_PATH = '/connections/callback';

/** An in-app path that is safe to navigate to after the callback (no other origins, no protocol-relative URLs). */
export function safeNextPath(raw: string | null): string | null {
  if (!raw || !raw.startsWith('/') || raw.startsWith('//') || raw.startsWith('/\\')) return null;
  return raw;
}

/** The `return_to` for `oauth/start`: the callback page, which then continues to `next` (e.g. the flow being edited). */
export function oauthReturnUrl(origin: string, next?: string): string {
  const safe = safeNextPath(next ?? null);
  return `${origin}${OAUTH_RETURN_PATH}${safe ? `?next=${encodeURIComponent(safe)}` : ''}`;
}
