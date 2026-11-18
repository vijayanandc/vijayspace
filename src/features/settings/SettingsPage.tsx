// Settings: the organisation (name, time zone), its users and invitations, run history retention, and (staff) a link
// to the platform page.

import { ChevronRight, Server, Settings } from 'lucide-react';
import { Link } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { useOrg } from '../../api/hooks';
import { ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { can, useRole, useSession } from '../auth';
import { OrgSettings } from './OrgSettings';
import { RetentionSettings } from './RetentionSettings';
import { UsersSettings } from './UsersSettings';

export function SettingsPage() {
  const org = useOrg();
  const owner = can(useRole(), 'org.manage');
  const staff = useSession()?.me.staff ?? true;
  return (
    <div className="page settings-page">
      <section className="list-card">
        <PageHeader icon={Settings} tone="#616161" label="Settings" title="Settings" meta={org.data ? `${org.data.name} · ${org.data.timezone}` : undefined} />
      </section>
      {org.isLoading && <Loading />}
      {org.error && <ErrorBox message={errorMessage(org.error)} />}
      {org.data && (
        <div className="settings-grid">
          <OrgSettings key={`${org.data.id}:${org.data.name}:${org.data.timezone}`} org={org.data} editable={owner} />
          <RetentionSettings org={org.data} editable={owner} />
          <UsersSettings />
          {staff && (
            <Link to="/settings/platform" className="card settings-link">
              <Server size={20} aria-hidden="true" />
              <span>
                <strong>Platform</strong>
                <span className="muted small">Servers, job queue lag and alerts (staff)</span>
              </span>
              <ChevronRight size={16} aria-hidden="true" />
            </Link>
          )}
        </div>
      )}
    </div>
  );
}
