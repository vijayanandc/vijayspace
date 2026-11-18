import { describe, expect, it } from 'vitest';

import { logtoConfig, redirectUris } from './config';
import { assignableRoles, can } from './permissions';

describe('sign-in configuration', () => {
  it('redirects back to this origin unless overridden at build time', () => {
    expect(redirectUris('http://127.0.0.1:3000', {})).toEqual({ signIn: 'http://127.0.0.1:3000/callback', signOut: 'http://127.0.0.1:3000' });
    expect(redirectUris('http://x', { VITE_LOGTO_REDIRECT_URI: ' https://app/cb ', VITE_LOGTO_POST_LOGOUT_REDIRECT_URI: '' })).toEqual({
      signIn: 'https://app/cb',
      signOut: 'http://x',
    });
  });

  it('asks for refresh tokens and the API resource when there is one', () => {
    const base = { endpoint: 'https://t.logto.app', app_id: 'web', resource: null, scopes: ['openid', 'profile', 'email'] };
    expect(logtoConfig(base)).toEqual({ endpoint: 'https://t.logto.app', appId: 'web', scopes: ['openid', 'profile', 'email', 'offline_access'] });
    expect(logtoConfig({ ...base, resource: 'https://api.weft' }).resources).toEqual(['https://api.weft']);
  });
});

describe('permissions mirror', () => {
  it('follows the server matrix, including "own only" deletes', () => {
    expect(can('viewer', 'flow.view')).toBe(true);
    expect(can('viewer', 'flow.edit')).toBe(false);
    expect(can('member', 'flow.delete', 'usr_1', 'usr_1')).toBe(true);
    expect(can('member', 'flow.delete', 'usr_2', 'usr_1')).toBe(false);
    expect(can('member', 'flow.delete', null, 'usr_1')).toBe(false);
    expect(can('admin', 'flow.delete', 'usr_2', 'usr_1')).toBe(true);
    expect(can('admin', 'org.manage')).toBe(false);
    expect(can('owner', 'org.manage')).toBe(true);
  });

  it('lets owners give any role and admins any but owner', () => {
    expect(assignableRoles('owner')).toEqual(['owner', 'admin', 'member', 'viewer']);
    expect(assignableRoles('admin')).toEqual(['admin', 'member', 'viewer']);
    expect(assignableRoles('admin', 'owner')).toEqual([]);
    expect(assignableRoles('member')).toEqual([]);
  });
});
