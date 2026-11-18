// Settings: the organisation (name, time zone), its users and invitations, and run history retention.

import { Settings } from 'lucide-react';

import { errorMessage } from '../../api/client';
import { useOrg } from '../../api/hooks';
import { ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { can, useRole } from '../auth';
import { OrgSettings } from './OrgSettings';
import { RetentionSettings } from './RetentionSettings';
import { UsersSettings } from './UsersSettings';

export function SettingsPage() {
  const org = useOrg();
  const owner = can(useRole(), 'org.manage');
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
        </div>
      )}
    </div>
  );
}
