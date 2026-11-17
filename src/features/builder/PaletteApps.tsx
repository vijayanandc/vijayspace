// Palette "Apps" tab: app tiles, drill-in to an app's actions, and search results grouped by app.

import { ChevronLeft, ChevronRight } from 'lucide-react';

import type { ConnectorManifest } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { PaletteItem } from './PaletteItem';

function Actions({ manifest, actions }: { manifest: ConnectorManifest; actions: ConnectorManifest['actions'] }) {
  return (
    <>
      {actions.map((a) => (
        <PaletteItem key={a.key} item={{ kind: 'action', connector: manifest.key, action: a.key }} icon={manifest.key} label={a.name} description={a.description} />
      ))}
    </>
  );
}

interface Props {
  manifests: ConnectorManifest[];
  query: string;
  drill: string | null;
  onDrill: (key: string | null) => void;
}

export function PaletteApps({ manifests, query, drill, onDrill }: Props) {
  const apps = manifests.filter((m) => m.actions.length > 0);
  const q = query.trim().toLowerCase();

  if (q) {
    const groups = apps
      .map((m) => ({ manifest: m, actions: m.name.toLowerCase().includes(q) ? m.actions : m.actions.filter((a) => a.name.toLowerCase().includes(q)) }))
      .filter((g) => g.actions.length > 0);
    if (groups.length === 0) return <p className="palette-empty">No apps or actions match “{query.trim()}”.</p>;
    return (
      <>
        {groups.map(({ manifest, actions }) => (
          <section key={manifest.key} className="palette-app">
            <h3 className="palette-group">
              <AppIcon app={manifest.key} size={16} decorative />
              {manifest.name}
            </h3>
            <Actions manifest={manifest} actions={actions} />
          </section>
        ))}
      </>
    );
  }

  const current = drill ? apps.find((m) => m.key === drill) : undefined;
  if (current) {
    return (
      <section className="palette-app">
        <button type="button" className="link-btn palette-back" onClick={() => onDrill(null)}>
          <ChevronLeft size={14} aria-hidden="true" />
          All apps
        </button>
        <div className="palette-app-head">
          <AppIcon app={current.key} name={current.name} size={40} decorative />
          <div>
            <div className="palette-app-name">{current.name}</div>
            <div className="palette-app-desc">{current.description}</div>
          </div>
        </div>
        <h3 className="palette-group">Actions · {current.actions.length}</h3>
        <Actions manifest={current} actions={current.actions} />
      </section>
    );
  }

  return (
    <div className="app-tiles">
      {apps.map((m) => (
        <button key={m.key} type="button" className="app-tile" onClick={() => onDrill(m.key)} title={m.description}>
          <AppIcon app={m.key} size={32} decorative />
          <span className="app-tile-name">{m.name}</span>
          <span className="app-tile-meta">
            {m.actions.length} action{m.actions.length === 1 ? '' : 's'}
            <ChevronRight size={12} aria-hidden="true" />
          </span>
        </button>
      ))}
    </div>
  );
}
