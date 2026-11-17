// One draggable / clickable palette entry (a logic step or an app action).

import { GripVertical } from 'lucide-react';
import type { DragEvent } from 'react';

import { AppIcon } from '../../components/AppIcon';
import { useBuilder } from './BuilderContext';
import { type PaletteItem as Item, writePaletteDrag } from './model/factories';

interface Props {
  item: Item;
  /** AppIcon key. */
  icon: string;
  label: string;
  description: string;
}

export function PaletteItem({ item, icon, label, description }: Props) {
  const { insertItem } = useBuilder();
  const onDragStart = (e: DragEvent) => writePaletteDrag(e.dataTransfer, item);
  return (
    <button type="button" className="palette-item" draggable onDragStart={onDragStart} onClick={() => insertItem(item)} title={`${description}\nDrag onto a + or click to add`}>
      <AppIcon app={icon} size={28} decorative />
      <span className="palette-item-text">
        <span className="palette-label">{label}</span>
        {description && <span className="palette-desc">{description}</span>}
      </span>
      <GripVertical size={14} className="palette-grip" aria-hidden="true" />
    </button>
  );
}
