// Fields shown only for some values of another field, as the server decides.

import { describe, expect, it } from 'vitest';

import type { FieldSpec } from '../api/types';
import { visibleFields } from './visibility';

const fields: FieldSpec[] = [
  { key: 'auth', label: 'Authentication', type: 'select', default: 'api_key_header' },
  { key: 'api_key', label: 'API key', type: 'password', show_when: { field: 'auth', in: ['api_key_header', 'api_key_query'] } },
  { key: 'token', label: 'Token', type: 'password', show_when: { field: 'auth', in: ['bearer'] } },
  { key: 'base_url', label: 'Base URL', type: 'string' },
];

describe('field visibility', () => {
  it('shows a field only while its controlling field holds one of its values', () => {
    expect(visibleFields(fields, { auth: 'bearer' }).map((f) => f.key)).toEqual(['auth', 'token', 'base_url']);
    expect(visibleFields(fields, { auth: 'api_key_query' }).map((f) => f.key)).toEqual(['auth', 'api_key', 'base_url']);
  });

  it('uses the default when the controlling field has no value yet', () => {
    expect(visibleFields(fields, {}).map((f) => f.key)).toEqual(['auth', 'api_key', 'base_url']);
  });
});
