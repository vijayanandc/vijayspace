// Settings: the organization (read-only for now) and run history retention.

import { Building2, Settings } from 'lucide-react';

import { errorMessage } from '../../api/client';
import { useOrg } from '../../api/hooks';
import { ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { RetentionSettings } from './RetentionSettings';

export function SettingsPage() {
  const org = useOrg();
  return (
    <div className="page settings-page">
      <section className="list-card">
        <PageHeader icon={Settings} tone="#616161" label="Settings" title="Settings" meta={org.data ? `${org.data.name} · ${org.data.timezone}` : undefined} />
      </section>
      {org.isLoading && <Loading />}
      {org.error && <ErrorBox message={errorMessage(org.error)} />}
      {org.data && (
        <div className="settings-grid">
          <RetentionSettings org={org.data} />
          <section className="card settings-card" aria-labelledby="org-title">
            <header className="settings-card-head">
              <span className="settings-icon" style={{ background: '#0f6cbd' }} aria-hidden="true">
                <Building2 size={18} />
              </span>
              <div>
                <h2 id="org-title">Organization</h2>
                <p className="muted small">Read-only for now.</p>
              </div>
            </header>
            <dl className="settings-facts">
              <div>
                <dt>Name</dt>
                <dd>{org.data.name}</dd>
              </div>
              <div>
                <dt>Time zone</dt>
                <dd>{org.data.timezone}</dd>
              </div>
              <div>
                <dt>Organization ID</dt>
                <dd className="mono">{org.data.id}</dd>
              </div>
            </dl>
          </section>
        </div>
      )}
    </div>
  );
}
