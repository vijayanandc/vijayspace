import { describe, expect, it } from 'vitest';

import { sampleDefinition } from '../model/fixtures';
import { INSERT_SIZE, layoutDefinition } from '../model/layout';
import { CARD_WIDTH, COLUMN_PITCH, ROW_STRETCH, toViewEdges, toViewNodes } from './viewLayout';

describe('viewLayout', () => {
  const { nodes, edges } = layoutDefinition(sampleDefinition());
  const view = new Map(toViewNodes(nodes).map((n) => [n.id, n]));
  const centre = (id: string) => {
    const n = view.get(id);
    if (!n) throw new Error(`missing ${id}`);
    const w = n.type === 'insert' ? INSERT_SIZE : n.type === 'end' ? 80 : CARD_WIDTH;
    return n.position.x + w / 2;
  };

  it('keeps the trigger, root inserts and End on one centre line', () => {
    expect(centre('trigger')).toBeCloseTo(0);
    expect(centre('ins:root:0')).toBeCloseTo(0);
    expect(centre('end')).toBeCloseTo(0);
  });

  it('spreads decision columns by the column pitch and keeps cards centred on their insert points', () => {
    expect(centre('step:thank') - centre('step:notify')).toBeCloseTo(2 * COLUMN_PITCH);
    expect(centre('step:notify')).toBeCloseTo(centre('ins:b:dec:big:0'));
  });

  it('stretches rows without reordering them', () => {
    const trigger = view.get('trigger');
    const first = view.get('ins:root:0');
    expect(trigger?.position.y).toBe(0);
    expect(first?.position.y).toBeCloseTo((nodes.find((n) => n.id === 'ins:root:0')?.position.y ?? 0) * ROW_STRETCH);
  });

  it('labels branch edges and puts arrowheads only on edges into cards', () => {
    const styled = toViewEdges(edges);
    const branch = styled.find((e) => e.label === 'Big');
    expect(branch?.type).toBe('branch');
    expect(styled.filter((e) => e.target.startsWith('ins:')).every((e) => e.markerEnd === undefined)).toBe(true);
    expect(styled.find((e) => e.target === 'end')?.markerEnd).toBeDefined();
  });
});
