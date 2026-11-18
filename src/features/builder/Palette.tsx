// Left panel (Zoho Flow style): Apps | Logic tabs with search. Drag an item onto a "+" or click to insert.

import { MousePointerClick } from 'lucide-react';
import { useState } from 'react';

import { SearchBox } from '../../components/SearchBox';
import { Tabs } from '../../components/Tabs';
import { useBuilder } from './BuilderContext';
import { LOGIC_ITEMS } from './model/factories';
import { PaletteApps } from './PaletteApps';
import { PaletteItem } from './PaletteItem';

type PaletteTab = 'apps' | 'logic';

function LogicList({ query }: { query: string }) {
  const q = query.trim().toLowerCase();
  const items = LOGIC_ITEMS.filter((l) => !q || l.label.toLowerCase().includes(q) || l.description.toLowerCase().includes(q));
  if (items.length === 0) return <p className="palette-empty">No logic steps match “{query.trim()}”.</p>;
  const groups = [...new Set(items.map((l) => l.group))];
  return (
    <>
      {groups.map((group) => (
        <section key={group}>
          <h3 className="palette-group">{group}</h3>
          {items
            .filter((l) => l.group === group)
            .map((l) => (
              <PaletteItem key={l.type} item={{ kind: 'logic', type: l.type }} icon={`logic:${l.type}`} label={l.label} description={l.description} />
            ))}
        </section>
      ))}
    </>
  );
}

export function Palette() {
  const { manifests, activePoint, setActivePoint } = useBuilder();
  const [tab, setTab] = useState<PaletteTab>('apps');
  const [query, setQuery] = useState('');
  const [drill, setDrill] = useState<string | null>(null);

  return (
    <aside className="palette" aria-label="Steps palette">
      <div className="palette-head">
        <Tabs
          label="Step types"
          tabs={[
            { key: 'apps', label: 'Apps' },
            { key: 'logic', label: 'Logic' },
          ]}
          value={tab}
          onChange={setTab}
          className="palette-tabs"
        />
        <SearchBox
          label="Search palette"
          placeholder={tab === 'apps' ? 'Search apps and actions' : 'Search logic'}
          value={query}
          onChange={setQuery}
        />
      </div>
      {activePoint && (
        <div className="palette-banner" role="status">
          <MousePointerClick size={16} aria-hidden="true" />
          <span>Pick an item to insert it at the selected +.</span>
          <button type="button" className="link-btn" onClick={() => setActivePoint(null)}>
            Cancel
          </button>
        </div>
      )}
      <div className="palette-body">
        {tab === 'apps' ? <PaletteApps manifests={manifests} query={query} drill={drill} onDrill={setDrill} /> : <LogicList query={query} />}
      </div>
      <p className="palette-foot">Drag onto a + on the canvas, or click to add at the end.</p>
    </aside>
  );
}
