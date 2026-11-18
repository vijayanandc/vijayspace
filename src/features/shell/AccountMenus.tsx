// Header popovers: the organisation switcher (orgs, invitations waiting, "Create organisation") and the profile
// (who is signed in, their role, sign out). Both are disclosure popovers of buttons, not menus. Without a
// session (sign-in turned off) they show the org name only.

import { useQueryClient } from '@tanstack/react-query';
import { Building2, Check, ChevronDown, LogOut, Plus, UserRound } from 'lucide-react';
import { useState } from 'react';

import { qk, useOrg } from '../../api/hooks';
import type { Me, Membership } from '../../api/tenancy';
import { Dialog } from '../../components/Dialog';
import { Popover } from '../../components/Popover';
import { CreateOrgForm, InvitationList, ROLE_LABELS, useSession } from '../auth';
import { initials } from './nav';

/** Adds an org the user just created or joined to the cached `/me` (before switching to it). */
function addOrg(old: Me | undefined, org: Membership): Me | undefined {
  if (!old) return old;
  return { ...old, orgs: [...old.orgs.filter((o) => o.id !== org.id), org], invitations: old.invitations.filter((i) => i.org_id !== org.id) };
}

export function OrgSwitcher() {
  const session = useSession();
  const { data: org } = useOrg();
  const [creating, setCreating] = useState(false);
  const qc = useQueryClient();
  const current = session?.org ?? org;
  const name = current?.name ?? 'Workspace';
  if (!session?.signedIn) {
    return (
      <span className="header-org" title={current?.timezone ? `${name} · ${current.timezone}` : name}>
        {name}
      </span>
    );
  }
  const joined = (next: Membership) => {
    qc.setQueryData<Me>(qk.me, (old) => addOrg(old, next));
    session.switchOrg(next.id);
  };
  const waiting = session.me.invitations.length;
  return (
    <>
      <Popover
        className="org-menu"
        trigger={(open, toggle) => (
          <button type="button" className="header-org header-org-btn" aria-expanded={open} onClick={toggle} title="Switch organisation">
            <Building2 size={16} aria-hidden="true" />
            <span className="header-org-name">{name}</span>
            {waiting > 0 && (
              <span className="count-badge">
                <span aria-hidden="true">{waiting}</span>
                <span className="sr-only">{`${waiting} invitation${waiting === 1 ? '' : 's'} waiting`}</span>
              </span>
            )}
            <ChevronDown size={14} aria-hidden="true" />
          </button>
        )}
      >
        {(close) => (
          <div className="menu" role="group" aria-label="Organisations">
            <div className="popover-title">Organisations</div>
            {session.me.orgs.map((o) => (
              <button
                key={o.id}
                type="button"
                aria-current={o.id === session.org.id ? 'true' : undefined}
                className="menu-item org-item"
                onClick={() => {
                  close();
                  if (o.id !== session.org.id) session.switchOrg(o.id);
                }}
              >
                <span className="org-avatar" aria-hidden="true">
                  {initials(o.name)}
                </span>
                <span className="org-item-text">
                  <span>{o.name}</span>
                  <span className="muted small">{ROLE_LABELS[o.role].label}</span>
                </span>
                {o.id === session.org.id && <Check size={16} aria-label="Current" />}
              </button>
            ))}
            {waiting > 0 && (
              <>
                <div className="menu-sep" role="separator" />
                <div className="popover-title">Invitations</div>
                <InvitationList
                  invitations={session.me.invitations}
                  onAccepted={(next) => {
                    close();
                    joined(next);
                  }}
                />
              </>
            )}
            {session.me.can_create_orgs && (
              <>
                <div className="menu-sep" role="separator" />
                <button
                  type="button"
                  className="menu-item"
                  onClick={() => {
                    close();
                    setCreating(true);
                  }}
                >
                  <Plus size={16} aria-hidden="true" />
                  Create organisation
                </button>
              </>
            )}
          </div>
        )}
      </Popover>
      <Dialog title="Create organisation" open={creating} onClose={() => setCreating(false)} subtitle="You will be its owner. Invite people from Settings → Users.">
        <CreateOrgForm
          onCancel={() => setCreating(false)}
          onCreated={(created) => {
            setCreating(false);
            joined(created);
          }}
        />
      </Dialog>
    </>
  );
}

export function ProfileMenu() {
  const session = useSession();
  const user = session?.me.user;
  const label = user?.name ?? user?.email ?? 'Local developer';
  return (
    <Popover
      className="profile-menu"
      trigger={(open, toggle) => (
        <button type="button" className="avatar avatar-btn" aria-label={`Account: ${label}`} aria-expanded={open} onClick={toggle}>
          {initials(label)}
        </button>
      )}
    >
      {(close) => (
        <div className="menu" role="group" aria-label="Account">
          <div className="profile-head">
            <span className="avatar avatar-lg" aria-hidden="true">
              {initials(label)}
            </span>
            <div>
              <div className="profile-name">{label}</div>
              {user?.email && <div className="muted small">{user.email}</div>}
              {session && (
                <div className="muted small">
                  {ROLE_LABELS[session.org.role].label} · {session.org.name}
                </div>
              )}
            </div>
          </div>
          {!session?.signedIn && (
            <p className="popover-note">
              <UserRound size={14} aria-hidden="true" /> Sign-in is turned off on this server (development).
            </p>
          )}
          {session?.signedIn && (
            <>
              <div className="menu-sep" role="separator" />
              <button
                type="button"
                className="menu-item"
                onClick={() => {
                  close();
                  session.signOut();
                }}
              >
                <LogOut size={16} aria-hidden="true" />
                Sign out
              </button>
            </>
          )}
        </div>
      )}
    </Popover>
  );
}
