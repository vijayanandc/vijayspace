// Main navigation areas, shared by the left rail and the app launcher.

import type { LucideIcon } from 'lucide-react';
import { History, LayoutGrid, Plug, Settings, Workflow } from 'lucide-react';

export interface NavItem {
  to: string;
  label: string;
  description: string;
  icon: LucideIcon;
  /** Tile color in the app launcher. */
  tone: string;
}

export const NAV: NavItem[] = [
  { to: '/flows', label: 'Flows', description: 'Build and manage your automations', icon: Workflow, tone: '#0f6cbd' },
  { to: '/recipes', label: 'Gallery', description: 'Install prebuilt integrations', icon: LayoutGrid, tone: '#8764b8' },
  { to: '/connections', label: 'Connections', description: 'Accounts your flows use', icon: Plug, tone: '#0b7a75' },
  { to: '/history', label: 'History', description: 'Monitor runs and fix failures', icon: History, tone: '#ca5010' },
];

/** Settings: in the app launcher and behind the header's gear, not in the rail. */
export const SETTINGS_NAV: NavItem = { to: '/settings', label: 'Settings', description: 'Organization and run history retention', icon: Settings, tone: '#616161' };

/** Everything the app launcher lists. */
export const LAUNCHER: NavItem[] = [...NAV, SETTINGS_NAV];

/** Up to two initials for an avatar ("Default org" → "DO"). */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const letters = parts.length > 1 ? `${parts[0]?.charAt(0) ?? ''}${parts[1]?.charAt(0) ?? ''}` : (parts[0] ?? '?').slice(0, 2);
  return letters.toUpperCase() || '?';
}
