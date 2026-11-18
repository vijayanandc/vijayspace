import { describe, expect, it } from 'vitest';

import { sha256 } from './sha256';
import { ensureSubtleDigest } from './webCrypto';

const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
const text = (s: string) => new TextEncoder().encode(s);

describe('SHA-256 for plain-HTTP sign-in', () => {
  it('matches the FIPS 180-4 test vectors', () => {
    expect(hex(sha256(text('')))).toBe('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
    expect(hex(sha256(text('abc')))).toBe('ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
    expect(hex(sha256(text('abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq')))).toBe(
      '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1',
    );
    expect(hex(sha256(text('a'.repeat(1000))))).toBe('41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3');
  });

  it('installs a digest only where crypto.subtle is missing, and it agrees with Web Crypto', async () => {
    const real = globalThis.crypto;
    expect(ensureSubtleDigest(globalThis)).toBe(false);
    const insecure = { crypto: { getRandomValues: real.getRandomValues.bind(real) } as unknown as Crypto };
    expect(ensureSubtleDigest(insecure)).toBe(true);
    const verifier = text('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk');
    const ours = new Uint8Array(await insecure.crypto.subtle.digest('SHA-256', verifier));
    const theirs = new Uint8Array(await real.subtle.digest('SHA-256', verifier));
    expect(hex(ours)).toBe(hex(theirs));
    await expect(insecure.crypto.subtle.digest('SHA-1', verifier)).rejects.toThrow(/Only SHA-256/);
  });
});
