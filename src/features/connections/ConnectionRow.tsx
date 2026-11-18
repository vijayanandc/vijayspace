// One row of the connections table: app, status, today's usage (opens "Limits & usage") and row actions.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Gauge, KeyRound, PlugZap, Trash2 } from 'lucide-react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Connection, ConnectionUsage, ConnectorManifest, JsonObject } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { StatusBadge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { OverflowMenu } from '../../components/Popover';
import { useToast } from '../../components/Toast';
import { formatTime, relativeTime } from '../../lib/format';
import { oauthReturnUrl } from './oauth';
import { usageSummary } from './limits';
import { UsageMeter } from './UsageMeter';
import { can } from '../auth/permissions';
import { useRole, useUserId } from '../auth/session';

/** The connection's non-secret auth settings (e.g. region), resent when signing in again. */
function oauthSettings(connection: Connection, manifest: ConnectorManifest | undefined): JsonObject {
  const out: JsonObject = {};
  for (const f of manifest?.auth.fields ?? []) {
    const v = connection.config[f.key];
    if (!f.secret && v !== undefined) out[f.key] = v;
  }
  return out;
}

interface RowProps {
  connection: Connection;
  manifest: ConnectorManifest | undefined;
  usage: ConnectionUsage | undefined;
  onLimits: () => void;
}

export function ConnectionRow({ connection, manifest, usage, onLimits }: RowProps) {
  const role = useRole();
  const canUse = can(role, 'connection.use');
  const canManage = can(role, 'connection.create');
  const canDelete = can(role, 'connection.delete', connection.created_by, useUserId());
  const qc = useQueryClient();
  const toast = useToast();
  const isOauth = manifest?.auth.kind === 'oauth2';
  // Signs in again for this connection (`connection_id`), e.g. when it needs re-authorisation.
  const reconnect = useMutation({
    mutationFn: async () => {
      const res = await endpoints.oauthStart({
        connector: connection.connector,
        name: connection.name,
        connection_id: connection.id,
        fields: oauthSettings(connection, manifest),
        return_to: oauthReturnUrl(window.location.origin),
      });
      window.location.assign(res.authorize_url);
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const test = useMutation({
    mutationFn: () => endpoints.testConnection(connection.id),
    onSuccess: (res) => {
      toast.show(res.ok ? 'success' : 'error', res.ok ? `“${connection.name}” works` : `Test failed: ${res.error.message}`);
      void qc.invalidateQueries({ queryKey: qk.connections });
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: () => endpoints.deleteConnection(connection.id),
    onSuccess: () => {
      toast.show('success', `Deleted “${connection.name}”`);
      void qc.invalidateQueries({ queryKey: qk.connections });
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const confirmDelete = () => {
    if (window.confirm(`Delete connection “${connection.name}”?`)) remove.mutate();
  };
  return (
    <tr>
      <td className="col-name">
        <div className="name-cell">
          <AppIcon app={connection.connector} name={manifest?.name ?? connection.connector} size={28} decorative />
          <div>
            <div className="row-title">{connection.name}</div>
            <div className="row-sub mono">{connection.id}</div>
          </div>
        </div>
      </td>
      <td>{manifest?.name ?? connection.connector}</td>
      <td>
        <StatusBadge status={connection.status} />
      </td>
      <td className="col-usage">
        <button type="button" className="usage-cell" onClick={onLimits} aria-label={usage ? `Limits and usage of ${connection.name}: ${usageSummary(usage)}` : `Limits and usage of ${connection.name}`} title="Limits & usage">
          {usage ? <UsageMeter usage={usage} /> : <span className="cell-muted">—</span>}
        </button>
      </td>
      <td title={formatTime(connection.created_at)}>{relativeTime(connection.created_at)}</td>
      <td className="col-actions">
        <span className="row-actions">
          {canManage && isOauth && connection.status === 'needs_reauth' && (
            <Button size="sm" variant="ghost" icon={KeyRound} disabled={reconnect.isPending} onClick={() => reconnect.mutate()}>
              Reconnect
            </Button>
          )}
          {canUse && (
            <Button size="sm" variant="ghost" icon={PlugZap} disabled={test.isPending} onClick={() => test.mutate()}>
              {test.isPending ? 'Testing…' : 'Test'}
            </Button>
          )}
          <OverflowMenu
            label={`More actions for ${connection.name}`}
            items={[
              ...(canUse ? [{ label: 'Test connection', icon: PlugZap, onSelect: () => test.mutate(), disabled: test.isPending }] : []),
              { label: 'Limits & usage', icon: Gauge, onSelect: onLimits },
              ...(canManage && isOauth ? [{ label: 'Reconnect (sign in again)', icon: KeyRound, onSelect: () => reconnect.mutate(), disabled: reconnect.isPending }] : []),
              ...(canDelete ? [{ label: 'Delete', icon: Trash2, danger: true, onSelect: confirmDelete }] : []),
            ]}
          />
        </span>
      </td>
    </tr>
  );
}
