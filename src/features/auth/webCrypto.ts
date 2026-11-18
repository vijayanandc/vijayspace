// Sign-in over plain HTTP (e.g. http://192.168.1.20:3000): browsers expose `crypto.subtle` only in
// secure contexts (HTTPS, localhost, 127.0.0.1), and the Logto SDK needs it once, to hash the PKCE
// code verifier with SHA-256. Where it is missing, this installs a `digest('SHA-256')` fallback;
// random values still come from `crypto.getRandomValues`, which every context has.

import { sha256 } from './sha256';

type DigestInput = ArrayBuffer | ArrayBufferView;

function bytes(data: DigestInput): Uint8Array {
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
}

function algorithmName(algorithm: AlgorithmIdentifier): string {
  return (typeof algorithm === 'string' ? algorithm : algorithm.name).toUpperCase();
}

/** Installs the SHA-256 fallback when `crypto.subtle` is missing. Returns whether it did. */
export function ensureSubtleDigest(target: { crypto?: Crypto } = globalThis): boolean {
  const crypto = target.crypto;
  if (!crypto || crypto.subtle !== undefined) return false;
  const subtle = {
    digest: async (algorithm: AlgorithmIdentifier, data: DigestInput): Promise<ArrayBuffer> => {
      if (algorithmName(algorithm) !== 'SHA-256') throw new Error('Only SHA-256 is available on this insecure (HTTP) origin');
      const digest = sha256(bytes(data));
      return digest.buffer.slice(digest.byteOffset, digest.byteOffset + digest.byteLength) as ArrayBuffer;
    },
  };
  Object.defineProperty(crypto, 'subtle', { value: subtle, configurable: true });
  return true;
}
