// Settings → Users: the organisation's members and their roles, and invitations by email. Owners and admins manage
// people (admins never touch owners); everyone sees who is in the organisation and may leave it.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { MailPlus, Users } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { qk } from '../../api/hooks';
import { type Member, type Role, tenancy } from '../../api/tenancy';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { ErrorBox, Loading } from '../../components/Empty';
import { useToast } from '../../components/Toast';
import { formatTime } from '../../lib/format';
import { ROLE_LABELS, assignableRoles, can } from '../auth/permissions';
import { useSession } from '../auth/session';
import { InviteDialog } from './InviteDialog';
import { PendingInvitations } from './PendingInvitations';

function MemberRow({ member, actor, me, lastOwner }: { member: Member; actor: Role; me: string | null; lastOwner: boolean }) {
  const qc = useQueryClient();
  const toast = useToast();
  const session = useSession();
  const isMe = member.user.id === me;
  const roles = assignableRoles(actor, member.role);
  const done = () => void qc.invalidateQueries({ queryKey: qk.members });
  const setRole = useMutation({
    mutationFn: (role: Role) => tenancy.setRole(member.user.id, role),
    onSuccess: (m) => {
      done();
      toast.show('success', `${m.user.name ?? m.user.email ?? 'Member'} is now ${ROLE_LABELS[m.role].label.toLowerCase()}`);
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: () => tenancy.removeMember(member.user.id),
    onSuccess: () => {
      if (isMe) {
        void qc.invalidateQueries({ queryKey: qk.me });
        session?.switchOrg(session.me.orgs.find((o) => o.id !== session.org.id)?.id ?? '');
        return;
      }
      done();
      toast.show('success', 'Removed from the organisation');
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const name = member.user.name ?? member.user.email ?? member.user.id;
  // The last owner can neither leave nor be removed (the server refuses it too).
  const canRemove = !(lastOwner && member.role === 'owner') && (isMe || (can(actor, 'members.manage') && roles.length > 0));
  const confirmRemove = () => {
    const question = isMe ? 'Leave this organisation? You will lose access to its flows.' : `Remove ${name} from the organisation?`;
    if (window.confirm(question)) remove.mutate();
  };
  return (
    <tr>
      <td>
        <div className="row-title">
          {name} {isMe && <Badge tone="info">You</Badge>}
        </div>
        <div className="row-sub">{member.user.email}</div>
      </td>
      <td>
        {roles.length > 0 && !isMe ? (
          <select className="input input-sm" aria-label={`Role of ${name}`} value={member.role} disabled={setRole.isPending} onChange={(e) => setRole.mutate(e.target.value as Role)}>
            {Array.from(new Set([member.role, ...roles])).map((r) => (
              <option key={r} value={r}>
                {ROLE_LABELS[r].label}
              </option>
            ))}
          </select>
        ) : (
          <span>{ROLE_LABELS[member.role].label}</span>
        )}
      </td>
      <td className="cell-muted">{formatTime(member.joined_at)}</td>
      <td className="col-actions">
        {canRemove && (
          <Button size="sm" variant="ghost" onClick={confirmRemove} disabled={remove.isPending}>
            {isMe ? 'Leave' : 'Remove'}
          </Button>
        )}
      </td>
    </tr>
  );
}

export function UsersSettings() {
  const session = useSession();
  const [inviting, setInviting] = useState(false);
  const members = useQuery({ queryKey: qk.members, queryFn: tenancy.members, enabled: Boolean(session?.signedIn) });
  const actor = session?.org.role ?? 'owner';
  const manage = can(actor, 'members.manage');
  const owners = (members.data ?? []).filter((m) => m.role === 'owner').length;
  return (
    <section className="card settings-card settings-wide" aria-labelledby="users-title">
      <header className="settings-card-head">
        <span className="settings-icon" style={{ background: '#8764b8' }} aria-hidden="true">
          <Users size={18} />
        </span>
        <div>
          <h2 id="users-title">Users</h2>
          <p className="muted small">People in this organisation and what they can do.</p>
        </div>
        {manage && session?.signedIn && (
          <Button variant="primary" icon={MailPlus} onClick={() => setInviting(true)} className="settings-head-action">
            Invite people
          </Button>
        )}
      </header>
      {!session?.signedIn && <p className="muted">Sign-in is turned off on this server, so there are no users to manage.</p>}
      {members.isLoading && <Loading />}
      {members.error && <ErrorBox message={errorMessage(members.error)} />}
      {members.data && (
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Role</th>
              <th>Joined</th>
              <th className="col-actions">
                <span className="sr-only">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {members.data.map((m) => (
              <MemberRow key={m.user.id} member={m} actor={actor} me={session?.me.user?.id ?? null} lastOwner={owners <= 1} />
            ))}
          </tbody>
        </table>
      )}
      {manage && session?.signedIn && <PendingInvitations actor={actor} />}
      <InviteDialog open={inviting} roles={assignableRoles(actor)} onClose={() => setInviting(false)} />
    </section>
  );
}
