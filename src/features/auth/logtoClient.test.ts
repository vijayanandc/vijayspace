import { describe, expect, it } from 'vitest';

import { canVerifySignatures, checkIdTokenClaims } from './logtoClient';

const b64url = (v: object) => btoa(JSON.stringify(v)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const token = (claims: object) => `${b64url({ alg: 'ES384' })}.${b64url(claims)}.sig`;
const NOW = 1_800_000_000;
const ISS = 'https://t.logto.app/oidc';

describe('ID tokens on plain-HTTP origins', () => {
  it('accepts a token for this app from this issuer, issued now', () => {
    expect(() => checkIdTokenClaims(token({ iss: ISS, aud: 'web', exp: NOW + 3600, iat: NOW }), ISS, 'web', NOW)).not.toThrow();
    expect(() => checkIdTokenClaims(token({ iss: ISS, aud: ['web', 'x'], exp: NOW + 3600, iat: NOW - 10 }), ISS, 'web', NOW)).not.toThrow();
  });

  it('refuses other issuers, other apps, expired or mistimed tokens and garbage', () => {
    const base = { iss: ISS, aud: 'web', exp: NOW + 3600, iat: NOW };
    for (const bad of [{ iss: 'https://evil/oidc' }, { aud: 'other' }, { exp: NOW - 301 }, { iat: NOW - 4000 }, { iat: undefined }]) {
      expect(() => checkIdTokenClaims(token({ ...base, ...bad }), ISS, 'web', NOW), JSON.stringify(bad)).toThrow(/not valid/);
    }
    expect(() => checkIdTokenClaims('garbage', ISS, 'web', NOW)).toThrow();
  });

  it('keeps signature checks wherever Web Crypto exists', () => {
    expect(canVerifySignatures()).toBe(true);
    expect(canVerifySignatures({ crypto: { getRandomValues: () => new Uint8Array() } as unknown as Crypto })).toBe(false);
  });
});
