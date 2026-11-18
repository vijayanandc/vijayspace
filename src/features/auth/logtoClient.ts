// The Logto client Weft signs in with. On secure origins (HTTPS, localhost, 127.0.0.1) it is the SDK's client
// unchanged. On plain-HTTP origins browsers withhold Web Crypto, so the SDK can neither hash the PKCE verifier
// (see webCrypto.ts) nor check the ID token's signature. There, ID tokens are checked by their claims only: they
// come straight from Logto's token endpoint over TLS in the code exchange, and OpenID Connect Core §3.1.3.7 allows
// TLS server validation in place of the signature check in that case. The server never trusts ID tokens anyway; it
// checks every access token with Logto itself. (Security-sensitive: reviewed decision, see docs/STATUS.md.)

import LogtoClient from '@logto/browser';
import type { LogtoConfig } from '@logto/react';

/** Seconds of clock difference tolerated (the SDK's default). */
const CLOCK_TOLERANCE = 300;

interface IdTokenClaims {
  iss?: unknown;
  aud?: unknown;
  exp?: unknown;
  iat?: unknown;
}

function decodeClaims(idToken: string): IdTokenClaims {
  const payload = idToken.split('.')[1];
  if (!payload) throw new Error('The ID token is malformed');
  const json = atob(payload.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(payload.length / 4) * 4, '='));
  const claims: unknown = JSON.parse(json);
  if (!claims || typeof claims !== 'object') throw new Error('The ID token is malformed');
  return claims as IdTokenClaims;
}

/** Checks an ID token's issuer, audience and times (not its signature). Throws when it is not for this app. */
export function checkIdTokenClaims(idToken: string, issuer: string, appId: string, nowSeconds: number): void {
  const c = decodeClaims(idToken);
  const audience = Array.isArray(c.aud) ? c.aud : [c.aud];
  const fail = (why: string) => {
    throw new Error(`The ID token is not valid: ${why}`);
  };
  if (c.iss !== issuer) fail('wrong issuer');
  if (!audience.includes(appId)) fail('wrong audience');
  if (typeof c.exp !== 'number' || c.exp + CLOCK_TOLERANCE < nowSeconds) fail('expired');
  if (typeof c.iat !== 'number' || Math.abs(c.iat - nowSeconds) > CLOCK_TOLERANCE) fail('issued at the wrong time');
}

/** Whether this page can verify signatures with Web Crypto (false on plain-HTTP origins). */
export function canVerifySignatures(target: { crypto?: Crypto } = globalThis): boolean {
  return typeof target.crypto?.subtle?.verify === 'function' && typeof target.crypto.subtle.importKey === 'function';
}

export class WeftLogtoClient extends LogtoClient {
  constructor(config: LogtoConfig, enableCache?: boolean) {
    super(config, enableCache);
    if (!canVerifySignatures()) {
      this.setJwtVerifier({
        verifyIdToken: async (idToken: string) => {
          const { issuer } = await this.getOidcConfig();
          checkIdTokenClaims(idToken, issuer, this.logtoConfig.appId, Math.floor(Date.now() / 1000));
        },
      });
    }
  }
}
