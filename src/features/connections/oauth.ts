// OAuth round trip helpers: where the provider sends the browser back to, and where the app goes next.

export const OAUTH_RETURN_PATH = '/connections/callback';

/** Characters browsers drop or reinterpret while parsing a URL (controls, whitespace, backslash). */
function hasUnsafeChar(raw: string): boolean {
  for (const ch of raw) {
    const code = ch.charCodeAt(0);
    if (code <= 0x20 || code === 0x7f || ch === '\\') return true;
  }
  return false;
}

/**
 * An in-app path that is safe to navigate to after the callback, or `null`.
 * Only same-origin, path-only values pass: browsers strip tabs and newlines while parsing, so
 * `/\t/evil.example` would otherwise become the protocol-relative `//evil.example`.
 */
export function safeNextPath(raw: string | null, origin: string = window.location.origin): string | null {
  if (!raw || !raw.startsWith('/') || raw.startsWith('//') || hasUnsafeChar(raw)) return null;
  let url: URL;
  try {
    url = new URL(raw, origin);
  } catch {
    return null;
  }
  if (url.origin !== origin) return null;
  return `${url.pathname}${url.search}${url.hash}`;
}

/** The `return_to` for `oauth/start`: the callback page, which then continues to `next` (e.g. the flow being edited). */
export function oauthReturnUrl(origin: string, next?: string): string {
  const safe = safeNextPath(next ?? null);
  return `${origin}${OAUTH_RETURN_PATH}${safe ? `?next=${encodeURIComponent(safe)}` : ''}`;
}
