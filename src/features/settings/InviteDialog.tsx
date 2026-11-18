// Invite people by email: they get a link, sign in with that address and accept.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { type FormEvent, useState } from 'react';

import { errorMessage } from '../../api/client';
import { qk } from '../../api/hooks';
import { type Role, tenancy } from '../../api/tenancy';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { Field } from '../../components/Field';
import { useToast } from '../../components/Toast';
import { ROLE_LABELS } from '../auth';

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export function InviteDialog({ open, roles, onClose }: { open: boolean; roles: Role[]; onClose: () => void }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [email, setEmail] = useState('');
  const [role, setRole] = useState<Role>(roles.includes('member') ? 'member' : (roles[0] ?? 'viewer'));
  const [touched, setTouched] = useState(false);
  const invite = useMutation({
    mutationFn: () => tenancy.invite(email.trim(), role),
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: qk.invitations });
      toast.show(
        res.email_sent ? 'success' : 'info',
        res.email_sent ? `Invitation sent to ${res.invitation.email}` : `Invited ${res.invitation.email}. The email could not be sent; share the link to Weft with them.`,
      );
      setEmail('');
      setTouched(false);
      onClose();
    },
  });
  const emailError = touched && !EMAIL.test(email.trim()) ? 'Enter a valid email address.' : undefined;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (EMAIL.test(email.trim())) invite.mutate();
  };
  return (
    <Dialog
      title="Invite people"
      subtitle="They sign in with this email address and accept the invitation. It expires in 14 days."
      open={open}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" type="submit" form="invite-form" disabled={invite.isPending}>
            {invite.isPending ? 'Sending…' : 'Send invitation'}
          </Button>
        </>
      }
    >
      <form id="invite-form" onSubmit={submit} noValidate>
        <Field label="Email address" required error={emailError}>
          {(id) => <input id={id} className="input" type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="off" />}
        </Field>
        <fieldset className="role-choices">
          <legend className="field-label">Role</legend>
          {roles.map((r) => (
            <label key={r} className="role-choice">
              <input type="radio" name="invite-role" value={r} checked={role === r} onChange={() => setRole(r)} />
              <span>
                <strong>{ROLE_LABELS[r].label}</strong>
                <span className="muted small">{ROLE_LABELS[r].description}</span>
              </span>
            </label>
          ))}
        </fieldset>
        {invite.error && (
          <p className="field-error-text" role="alert">
            {errorMessage(invite.error)}
          </p>
        )}
      </form>
    </Dialog>
  );
}
