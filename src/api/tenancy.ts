// Sign-in and tenancy contract (backend docs/api/README.md, "Sign-in and organisations"): the public sign-in
// configuration, the signed-in user, their orgs, members and invitations.

import { api } from './client';
import type { Org } from './types';

export type Role = 'owner' | 'admin' | 'member' | 'viewer';
export const ROLES: Role[] = ['owner', 'admin', 'member', 'viewer'];

/** What the web client needs to sign in with Logto (`null` = sign-in is turned off on this server). */
export interface SignInConfig {
  endpoint: string;
  app_id: string;
  resource: string | null;
  scopes: string[];
}

export interface User {
  id: string;
  email: string | null;
  email_verified: boolean;
  name: string | null;
  created_at: string;
}

/** An org with the caller's role in it. */
export type Membership = Org & { role: Role };

export interface Invitation {
  id: string;
  org_id: string;
  org_name: string;
  email: string;
  role: Role;
  invited_by: string | null;
  created_at: string;
  expires_at: string;
}

export interface Me {
  /** `null` when sign-in is turned off. */
  user: User | null;
  orgs: Membership[];
  invitations: Invitation[];
  /** An owner or admin of the operator's default org. */
  staff: boolean;
  /** Staff only, unless the server lets anyone signed in create organisations. */
  can_create_orgs: boolean;
}

export interface Member {
  org_id: string;
  user: User;
  role: Role;
  joined_at: string;
}

export interface Invited {
  invitation: Invitation;
  email_sent: boolean;
}

const enc = encodeURIComponent;

export const tenancy = {
  authConfig: () => api.get<{ sign_in: SignInConfig | null }>('/api/v1/auth/config'),
  me: () => api.get<Me>('/api/v1/me'),
  createOrg: (body: { name: string; timezone?: string }) => api.post<Membership>('/api/v1/orgs', body),
  updateOrg: (body: { name?: string; timezone?: string }) => api.patch<Membership>('/api/v1/org', body),
  acceptInvitation: (id: string) => api.post<Membership>(`/api/v1/me/invitations/${enc(id)}/accept`),
  declineInvitation: (id: string) => api.post<void>(`/api/v1/me/invitations/${enc(id)}/decline`),
  members: () => api.get<Member[]>('/api/v1/members'),
  setRole: (userId: string, role: Role) => api.patch<Member>(`/api/v1/members/${enc(userId)}`, { role }),
  removeMember: (userId: string) => api.del(`/api/v1/members/${enc(userId)}`),
  invitations: () => api.get<Invitation[]>('/api/v1/invitations'),
  invite: (email: string, role: Role) => api.post<Invited>('/api/v1/invitations', { email, role }),
  resendInvitation: (id: string) => api.post<Invited>(`/api/v1/invitations/${enc(id)}/resend`),
  revokeInvitation: (id: string) => api.del(`/api/v1/invitations/${enc(id)}`),
};
