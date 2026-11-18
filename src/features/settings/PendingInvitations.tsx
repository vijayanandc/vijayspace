// Open invitations of the organisation: resend the email or revoke.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { errorMessage } from '../../api/client';
import { qk } from '../../api/hooks';
import { type Role, tenancy } from '../../api/tenancy';
import { Button } from '../../components/Button';
import { useToast } from '../../components/Toast';
import { formatTime } from '../../lib/format';
import { ROLE_LABELS, assignableRoles } from '../auth';

export function PendingInvitations({ actor }: { actor: Role }) {
  const qc = useQueryClient();
  const toast = useToast();
  const list = useQuery({ queryKey: qk.invitations, queryFn: tenancy.invitations });
  const refresh = () => void qc.invalidateQueries({ queryKey: qk.invitations });
  const resend = useMutation({
    mutationFn: (id: string) => tenancy.resendInvitation(id),
    onSuccess: (r) => {
      refresh();
      toast.show(r.email_sent ? 'success' : 'info', r.email_sent ? `Sent again to ${r.invitation.email}` : 'Renewed; the email could not be sent');
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => tenancy.revokeInvitation(id),
    onSuccess: () => {
      refresh();
      toast.show('success', 'Invitation revoked');
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const open = list.data ?? [];
  if (open.length === 0) return null;
  const allowed = assignableRoles(actor);
  return (
    <div className="pending-invites">
      <h3 className="section-title">Invitations waiting</h3>
      <table className="table">
        <thead>
          <tr>
            <th>Email</th>
            <th>Role</th>
            <th>Expires</th>
            <th className="col-actions">
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {open.map((inv) => (
            <tr key={inv.id}>
              <td>{inv.email}</td>
              <td>{ROLE_LABELS[inv.role].label}</td>
              <td className="cell-muted">{formatTime(inv.expires_at)}</td>
              <td className="col-actions">
                {allowed.includes(inv.role) && (
                  <span className="row-actions">
                    <Button size="sm" variant="ghost" onClick={() => resend.mutate(inv.id)} disabled={resend.isPending}>
                      Resend
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => revoke.mutate(inv.id)} disabled={revoke.isPending}>
                      Revoke
                    </Button>
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
