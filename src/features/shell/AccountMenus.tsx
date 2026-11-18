// Header menus: the organisation switcher (with "Create organisation") and the profile menu (who is signed in,
// their role, sign out). Without a session (sign-in turned off) they show the org name only.

import { useQueryClient } from '@tanstack/react-query';
import { Building2, Check, ChevronDown, LogOut, Plus, UserRound } from 'lucide-react';
import { useState } from 'react';

import { qk, useOrg } from '../../api/hooks';
import type { Me } from '../../api/tenancy';
import { Dialog } from '../../components/Dialog';
import { Popover } from '../../components/Popover';
import { CreateOrgForm } from '../auth/CreateOrgForm';
import { ROLE_LABELS } from '../auth/permissions';
import { useSession } from '../auth/session';
import { initials } from './nav';

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
  return (
    <>
      <Popover
        className="org-menu"
        trigger={(open, toggle) => (
          <button type="button" className="header-org header-org-btn" aria-haspopup="menu" aria-expanded={open} onClick={toggle} title="Switch organisation">
            <Building2 size={16} aria-hidden="true" />
            <span className="header-org-name">{name}</span>
            <ChevronDown size={14} aria-hidden="true" />
          </button>
        )}
      >
        {(close) => (
          <div role="menu" className="menu" aria-label="Organisations">
            <div className="popover-title">Organisations</div>
            {session.me.orgs.map((o) => (
              <button
                key={o.id}
                type="button"
                role="menuitemradio"
                aria-checked={o.id === session.org.id}
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
                {o.id === session.org.id && <Check size={16} aria-hidden="true" />}
              </button>
            ))}
            <div className="menu-sep" role="separator" />
            <button
              type="button"
              role="menuitem"
              className="menu-item"
              onClick={() => {
                close();
                setCreating(true);
              }}
            >
              <Plus size={16} aria-hidden="true" />
              Create organisation
            </button>
          </div>
        )}
      </Popover>
      <Dialog title="Create organisation" open={creating} onClose={() => setCreating(false)} subtitle="You will be its owner. Invite people from Settings → Users.">
        <CreateOrgForm
          onCancel={() => setCreating(false)}
          onCreated={(created) => {
            setCreating(false);
            // Known to the session before switching, so it does not fall back to another org meanwhile.
            qc.setQueryData<Me>(qk.me, (old) => (old ? { ...old, orgs: [...old.orgs, created] } : old));
            session.switchOrg(created.id);
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
        <button type="button" className="avatar avatar-btn" aria-label={`Account: ${label}`} aria-haspopup="menu" aria-expanded={open} onClick={toggle}>
          {initials(label)}
        </button>
      )}
    >
      {(close) => (
        <div role="menu" className="menu" aria-label="Account">
          <div className="profile-head">
            <span className="avatar avatar-lg" aria-hidden="true">
              {initials(label)}
            </span>
            <div>
              <div className="profile-name">{label}</div>
              {user?.email && <div className="muted small">{user.email}</div>}
              {session && <div className="muted small">{ROLE_LABELS[session.org.role].label} · {session.org.name}</div>}
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
                role="menuitem"
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
