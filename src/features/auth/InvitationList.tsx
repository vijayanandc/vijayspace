// Invitations waiting for the signed-in user: accept to join the organisation, or decline.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { MailOpen } from 'lucide-react';

import { errorMessage } from '../../api/client';
import { qk } from '../../api/hooks';
import { type Invitation, type Membership, tenancy } from '../../api/tenancy';
import { Button } from '../../components/Button';
import { useToast } from '../../components/Toast';
import { formatTime } from '../../lib/format';
import { ROLE_LABELS } from './permissions';

export function InvitationList({ invitations, onAccepted }: { invitations: Invitation[]; onAccepted: (org: Membership) => void }) {
  const qc = useQueryClient();
  const toast = useToast();
  const accept = useMutation({
    mutationFn: (id: string) => tenancy.acceptInvitation(id),
    onSuccess: async (org) => {
      await qc.invalidateQueries({ queryKey: qk.me });
      toast.show('success', `You joined ${org.name}`);
      onAccepted(org);
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const decline = useMutation({
    mutationFn: (id: string) => tenancy.declineInvitation(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.me }),
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  if (invitations.length === 0) return null;
  return (
    <ul className="invite-list" aria-label="Invitations">
      {invitations.map((inv) => (
        <li key={inv.id} className="invite-item">
          <MailOpen size={18} aria-hidden="true" />
          <div className="invite-text">
            <strong>{inv.org_name}</strong>
            <span className="muted small">
              {inv.invited_by ? `${inv.invited_by} invited you` : 'You are invited'} as {ROLE_LABELS[inv.role].label.toLowerCase()} · expires{' '}
              {formatTime(inv.expires_at)}
            </span>
          </div>
          <div className="invite-actions">
            <Button size="sm" onClick={() => decline.mutate(inv.id)} disabled={decline.isPending || accept.isPending} aria-label={`Decline ${inv.org_name}`}>
              Decline
            </Button>
            <Button size="sm" variant="primary" onClick={() => accept.mutate(inv.id)} disabled={accept.isPending} aria-label={`Accept ${inv.org_name}`}>
              Accept
            </Button>
          </div>
        </li>
      ))}
    </ul>
  );
}
