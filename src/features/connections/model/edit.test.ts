// Editing a connection sends only what changed, and says which secrets a settings change needs again.

import { describe, expect, it } from 'vitest';

import type { FieldSpec } from '../../../api/types';
import { changedFields, editStart, secretsStillNeeded } from './edit';

const fields: FieldSpec[] = [
  { key: 'base_url', label: 'App URL', type: 'string' },
  { key: 'org_id', label: 'Org ID', type: 'string' },
  { key: 'api_token', label: 'API token', type: 'password', secret: true, required: true },
  { key: 'secret_headers', label: 'Secret headers', type: 'key_value', secret: true },
];
const config = { base_url: 'https://fsm.example.com', org_id: '1' };
const all = () => true;

describe('editing a connection', () => {
  it('starts from the stored settings and never from a secret', () => {
    expect(editStart(fields, { ...config, api_token: 'x', api_domain: 'y' })).toEqual(config);
  });

  it('sends nothing for a rename, the changed setting with its secret, or a new secret alone', () => {
    expect(changedFields(fields, config, { ...config, api_token: '' })).toBeNull();
    expect(changedFields(fields, config, { ...config, org_id: '2', api_token: 't' })).toEqual({ org_id: '2', api_token: 't' });
    expect(changedFields(fields, config, { ...config, api_token: 'rotated' })).toEqual({ api_token: 'rotated' });
    expect(changedFields(fields, config, { ...config, secret_headers: { 'X-Api-Secret': 's' } })).toEqual({ secret_headers: { 'X-Api-Secret': 's' } });
    expect(changedFields(fields, config, { ...config, secret_headers: {} })).toBeNull();
  });

  it('asks for the secrets again only when a setting changes', () => {
    expect(secretsStillNeeded(fields, config, { ...config }, all)).toEqual([]);
    // Only the required secret: optional secret headers are not demanded (the server drops them unless re-entered).
    expect(secretsStillNeeded(fields, config, { ...config, base_url: 'https://other.example.com' }, all)).toEqual(['API token']);
    expect(secretsStillNeeded(fields, config, { ...config, base_url: 'https://other.example.com', api_token: 't' }, all)).toEqual([]);
  });
});
