// Left palette: logic steps and every connector action. Drag onto a "+" or click to insert.

import { type DragEvent, useMemo, useState } from 'react';

import { useBuilder } from './BuilderContext';
import { LOGIC_ITEMS, type PaletteItem, writePaletteDrag } from './model/factories';

function Item({ item, icon, label, hint }: { item: PaletteItem; icon: string; label: string; hint: string }) {
  const { insertItem } = useBuilder();
  const onDragStart = (e: DragEvent) => writePaletteDrag(e.dataTransfer, item);
  return (
    <button type="button" className="palette-item" draggable onDragStart={onDragStart} onClick={() => insertItem(item)} title={hint}>
      <span className="palette-icon" aria-hidden="true">{icon}</span>
      <span className="palette-label">{label}</span>
    </button>
  );
}

export function Palette() {
  const { manifests, activePoint } = useBuilder();
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const q = query.trim().toLowerCase();

  const apps = useMemo(
    () =>
      manifests
        .map((m) => ({
          manifest: m,
          actions: m.actions.filter((a) => !q || a.name.toLowerCase().includes(q) || m.name.toLowerCase().includes(q)),
        }))
        .filter((g) => g.actions.length > 0),
    [manifests, q],
  );
  const logic = LOGIC_ITEMS.filter((l) => !q || l.label.toLowerCase().includes(q));

  return (
    <aside className="palette" aria-label="Steps palette">
      <input className="input" type="search" placeholder="Search apps and actions" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search palette" />
      <p className="muted small palette-hint">
        {activePoint ? 'Click an item to insert it at the highlighted +' : 'Drag onto a + on the canvas, or click to add at the end'}
      </p>
      {logic.length > 0 && (
        <section>
          <h3 className="palette-group">Logic</h3>
          {logic.map((l) => (
            <Item key={l.type} item={{ kind: 'logic', type: l.type }} icon={l.icon} label={l.label} hint={l.description} />
          ))}
        </section>
      )}
      <h3 className="palette-group">Apps</h3>
      {apps.map(({ manifest, actions }) => {
        const expanded = q !== '' || open[manifest.key] === true;
        return (
          <section key={manifest.key} className="palette-app">
            <button
              type="button"
              className="palette-app-header"
              aria-expanded={expanded}
              onClick={() => setOpen((prev) => ({ ...prev, [manifest.key]: !expanded }))}
            >
              <span aria-hidden="true">{manifest.icon}</span> {manifest.name}
              <span className="muted small"> ({actions.length})</span>
            </button>
            {expanded &&
              actions.map((a) => (
                <Item
                  key={a.key}
                  item={{ kind: 'action', connector: manifest.key, action: a.key }}
                  icon={manifest.icon}
                  label={a.name}
                  hint={a.description}
                />
              ))}
          </section>
        );
      })}
    </aside>
  );
}
