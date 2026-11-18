// The backend's permission matrix (doc 08 §2), mirrored to show or hide actions. The server always decides;
// this only keeps people from clicking buttons that would be refused.

import type { Role } from '../../api/tenancy';

export type Permission =
  | 'org.manage'
  | 'members.manage'
  | 'flow.view'
  | 'flow.edit'
  | 'flow.delete'
  | 'execution.view'
  | 'execution.rerun'
  | 'connection.create'
  | 'connection.use'
  | 'connection.delete';

/** `own` = only on things the user created. */
export type Access = 'yes' | 'own' | 'no';

const MATRIX: Record<Permission, Record<Role, Access>> = {
  'org.manage': { owner: 'yes', admin: 'no', member: 'no', viewer: 'no' },
  'members.manage': { owner: 'yes', admin: 'yes', member: 'no', viewer: 'no' },
  'flow.view': { owner: 'yes', admin: 'yes', member: 'yes', viewer: 'yes' },
  'flow.edit': { owner: 'yes', admin: 'yes', member: 'yes', viewer: 'no' },
  'flow.delete': { owner: 'yes', admin: 'yes', member: 'own', viewer: 'no' },
  'execution.view': { owner: 'yes', admin: 'yes', member: 'yes', viewer: 'yes' },
  'execution.rerun': { owner: 'yes', admin: 'yes', member: 'yes', viewer: 'no' },
  'connection.create': { owner: 'yes', admin: 'yes', member: 'yes', viewer: 'no' },
  'connection.use': { owner: 'yes', admin: 'yes', member: 'yes', viewer: 'no' },
  'connection.delete': { owner: 'yes', admin: 'yes', member: 'own', viewer: 'no' },
};

export function access(role: Role, permission: Permission): Access {
  return MATRIX[permission][role];
}

/** Whether `role` may do `permission` (on something created by `createdBy`, for "own only" permissions). */
export function can(role: Role, permission: Permission, createdBy?: string | null, me?: string | null): boolean {
  const a = access(role, permission);
  return a === 'yes' || (a === 'own' && Boolean(createdBy) && createdBy === me);
}

/** Roles `actor` may give (inviting, or changing someone whose role is `from`). */
export function assignableRoles(actor: Role, from?: Role): Role[] {
  if (actor === 'owner') return ['owner', 'admin', 'member', 'viewer'];
  if (actor === 'admin' && from !== 'owner') return ['admin', 'member', 'viewer'];
  return [];
}

export const ROLE_LABELS: Record<Role, { label: string; description: string }> = {
  owner: { label: 'Owner', description: 'Everything, including organisation settings' },
  admin: { label: 'Admin', description: 'Flows, connections and people (not owners)' },
  member: { label: 'Member', description: 'Builds and runs flows; deletes only their own' },
  viewer: { label: 'Viewer', description: 'Sees flows and runs, without run data' },
};
