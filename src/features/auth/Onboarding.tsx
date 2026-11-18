// First sign-in without an organisation: accept an invitation, or (staff, or when the server allows anyone) create
// an organisation and become its owner. Others are told to ask for an invitation.

import { useQueryClient } from '@tanstack/react-query';
import { LogOut } from 'lucide-react';

import { qk } from '../../api/hooks';
import type { Me, Membership } from '../../api/tenancy';
import { Button } from '../../components/Button';
import { WeftLogo } from '../../components/WeftLogo';
import { rememberOrg } from './session';
import { CreateOrgForm } from './CreateOrgForm';
import { InvitationList } from './InvitationList';

export function Onboarding({ me, signOut }: { me: Me; signOut: () => void }) {
  const qc = useQueryClient();
  const joined = (org: Membership) => {
    rememberOrg(org.id);
    void qc.invalidateQueries({ queryKey: qk.me });
  };
  const name = me.user?.name ?? me.user?.email ?? 'there';
  return (
    <div className="onboarding">
      <header className="onboarding-head">
        <WeftLogo size={28} />
        <span className="brand-name">Weft</span>
        <Button size="sm" variant="ghost" icon={LogOut} onClick={signOut} className="onboarding-signout">
          Sign out
        </Button>
      </header>
      <main className="onboarding-main">
        <h1>Welcome, {name}</h1>
        <p className="muted">
          {me.can_create_orgs
            ? 'Flows, connections and run history belong to an organisation. Join one you were invited to, or create your own.'
            : 'Flows, connections and run history belong to an organisation. Organisations are set up by your administrator: join one you were invited to.'}
        </p>
        {me.invitations.length > 0 && (
          <section className="card onboarding-card" aria-labelledby="invites-title">
            <h2 id="invites-title">Invitations</h2>
            <InvitationList invitations={me.invitations} onAccepted={joined} />
          </section>
        )}
        {me.can_create_orgs ? (
          <section className="card onboarding-card" aria-labelledby="create-title">
            <h2 id="create-title">Create an organisation</h2>
            <CreateOrgForm onCreated={joined} />
          </section>
        ) : (
          me.invitations.length === 0 && (
            <section className="card onboarding-card" aria-labelledby="ask-title">
              <h2 id="ask-title">No invitation yet</h2>
              <p className="muted">
                Ask your administrator to invite {me.user?.email ?? 'you'} to your organisation, then reload this page. You signed in successfully; there is just nothing for you here
                yet.
              </p>
            </section>
          )
        )}
        {me.user?.email && !me.user.email_verified && (
          <p className="muted small">
            Invitations are matched to your verified email address. Verify {me.user.email} with your sign-in provider to see them here.
          </p>
        )}
      </main>
    </div>
  );
}
