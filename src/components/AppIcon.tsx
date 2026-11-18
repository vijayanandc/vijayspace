// Neutral monogram / glyph tiles for connectors, trigger types and logic steps (never third-party logos).

import type { LucideIcon } from 'lucide-react';
import {
  ArrowLeftRight, CalendarClock, Globe, Hourglass, Mail, OctagonX, Play, Split, SquareFunction, Variable, WandSparkles, Webhook, Zap,
} from 'lucide-react';

interface TileSpec {
  bg: string;
  text?: string;
  icon?: LucideIcon;
}

const TILES: Record<string, TileSpec> = {
  zoho_crm: { bg: '#c8372d', text: 'CRM' },
  appblocks: { bg: '#5b47c4', text: 'AB' },
  autodesk_forma: { bg: '#0b7a75', text: 'F' },
  'core.http': { bg: '#2f5fb3', icon: Globe },
  'core.formatter': { bg: '#a65400', icon: WandSparkles },
  'core.email': { bg: '#0f6cbd', icon: Mail },
  'core.sync': { bg: '#2e7d32', icon: ArrowLeftRight },
  'trigger:webhook': { bg: '#0b6a8a', icon: Webhook },
  'trigger:schedule': { bg: '#7438b8', icon: CalendarClock },
  'trigger:manual': { bg: '#4f5bd5', icon: Play },
  'trigger:app': { bg: '#616161', icon: Zap },
  'logic:decision': { bg: '#8764b8', icon: Split },
  'logic:delay': { bg: '#ca5010', icon: Hourglass },
  'logic:set_variable': { bg: '#00727c', icon: Variable },
  'logic:stop': { bg: '#c4314b', icon: OctagonX },
  'logic:custom_function': { bg: '#3b4a6b', icon: SquareFunction },
  'core.script': { bg: '#3b4a6b', icon: SquareFunction },
};

const FALLBACK_COLORS = ['#5c6bc0', '#00897b', '#8e24aa', '#6d4c41', '#3949ab', '#00838f'];

/** Human names for the pseudo keys used by trigger types and logic steps. */
export const TILE_NAMES: Record<string, string> = {
  'trigger:webhook': 'Webhook',
  'trigger:schedule': 'Schedule',
  'trigger:manual': 'Manual',
  'trigger:app': 'App event',
  'logic:decision': 'Decision',
  'logic:delay': 'Delay',
  'logic:set_variable': 'Set variable',
  'logic:stop': 'Stop',
  'logic:custom_function': 'Custom function',
};

/** The tile for a key; unknown connectors get the first letter of their name on a stable color. */
export function tileFor(key: string, name?: string): TileSpec {
  const known = TILES[key];
  if (known) return known;
  const label = (name ?? key).trim() || '?';
  let hash = 0;
  for (const ch of key) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return { bg: FALLBACK_COLORS[hash % FALLBACK_COLORS.length] ?? '#616161', text: label.charAt(0).toUpperCase() };
}

/** Accent color of a tile (used for node accents). */
export function tileColor(key: string): string {
  return tileFor(key).bg;
}

interface Props {
  /** Connector key (`zoho_crm`), `trigger:<type>` or `logic:<type>`. */
  app: string;
  /** Accessible name and tooltip; defaults to the known name of the key. */
  name?: string;
  size?: number;
  /** Purely visual next to a visible name: hidden from assistive tech. */
  decorative?: boolean;
  className?: string;
}

export function AppIcon({ app, name, size = 32, decorative = false, className }: Props) {
  const label = name ?? TILE_NAMES[app] ?? app;
  const spec = tileFor(app, label);
  const Icon = spec.icon;
  // Tiny tiles (next to a visible name) keep only the first letter so the monogram stays legible.
  const text = size < 20 ? (spec.text ?? '').charAt(0) : (spec.text ?? '');
  const fontSize = Math.max(8, Math.round(size * (text.length >= 3 ? 0.36 : text.length === 2 ? 0.42 : 0.5) * 10) / 10);
  return (
    <span
      className={`app-icon${className ? ` ${className}` : ''}`}
      style={{ background: spec.bg, width: size, height: size, fontSize, borderRadius: size >= 32 ? 6 : 4, letterSpacing: text.length >= 3 ? '-0.04em' : undefined }}
      role={decorative ? undefined : 'img'}
      aria-label={decorative ? undefined : label}
      aria-hidden={decorative ? true : undefined}
      title={decorative ? undefined : label}
    >
      {Icon ? <Icon size={Math.round(size * 0.56)} strokeWidth={2} aria-hidden="true" /> : text}
    </span>
  );
}
