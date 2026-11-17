// Display labels for the trigger and steps: title, subtitle, kind and icon tile key.

import type { ConnectorManifest, Step, TriggerNode } from '../../../api/types';
import { LOGIC_ITEMS } from '../model/factories';

export interface NodeLabel {
  title: string;
  subtitle: string;
  /** Upper-case type label on cards ("Trigger", "Action", "Decision"…). */
  kind: string;
  /** AppIcon key. */
  icon: string;
  /** Accessible name of the icon tile. */
  iconName: string;
}

export function formatSeconds(seconds: number): string {
  if (seconds % 86_400 === 0 && seconds > 0) return `Wait ${seconds / 86_400} day(s)`;
  if (seconds % 3600 === 0 && seconds > 0) return `Wait ${seconds / 3600} hour(s)`;
  if (seconds % 60 === 0 && seconds > 0) return `Wait ${seconds / 60} minute(s)`;
  return `Wait ${seconds} s`;
}

export function triggerLabel(t: TriggerNode, manifests: ConnectorManifest[]): NodeLabel {
  switch (t.type) {
    case 'webhook':
      return { title: 'Webhook', subtitle: t.split_array ? 'Splits arrays into runs' : 'When a request arrives', kind: 'Trigger', icon: 'trigger:webhook', iconName: 'Webhook' };
    case 'schedule': {
      const s = t.schedule;
      const subtitle = s.kind === 'cron' ? `Cron ${s.expr}` : s.kind === 'interval' ? `Every ${s.minutes} min` : `Once at ${s.at}`;
      return { title: 'Schedule', subtitle, kind: 'Trigger', icon: 'trigger:schedule', iconName: 'Schedule' };
    }
    case 'app': {
      const m = manifests.find((x) => x.key === t.connector);
      const spec = m?.triggers.find((x) => x.key === t.trigger);
      return {
        title: spec?.name ?? (m ? m.name : 'App event'),
        subtitle: m ? `${m.name}${spec ? ` · ${spec.kind === 'instant' ? 'Instant' : 'Polling'}` : ' · Choose an event'}` : 'Choose an app and event',
        kind: 'Trigger',
        icon: m ? m.key : 'trigger:app',
        iconName: m?.name ?? 'App event',
      };
    }
    case 'manual':
      return { title: 'Manual', subtitle: 'Run now or via the API', kind: 'Trigger', icon: 'trigger:manual', iconName: 'Manual' };
  }
}

export function stepLabel(s: Step, manifests: ConnectorManifest[]): NodeLabel {
  if (s.type === 'action') {
    const m = manifests.find((x) => x.key === s.connector);
    const a = m?.actions.find((x) => x.key === s.action);
    return {
      title: s.name || a?.name || s.action,
      subtitle: `${m?.name ?? s.connector} · ${a?.name ?? s.action}`,
      kind: 'Action',
      icon: s.connector,
      iconName: m?.name ?? s.connector,
    };
  }
  const logic = LOGIC_ITEMS.find((l) => l.type === s.type);
  let subtitle = logic?.description ?? '';
  if (s.type === 'decision') subtitle = `${s.branches.length} branch${s.branches.length === 1 ? '' : 'es'} + otherwise`;
  if (s.type === 'delay') subtitle = s.until ? `Until ${s.until}` : formatSeconds(s.seconds ?? 0);
  if (s.type === 'set_variable') subtitle = s.assignments.map((a) => a.name).join(', ') || 'No variables';
  if (s.type === 'stop') subtitle = s.status === 'failed' ? 'End as failed' : 'End as succeeded';
  const label = logic?.label ?? s.type;
  return { title: s.name || label, subtitle, kind: label, icon: `logic:${s.type}`, iconName: label };
}
