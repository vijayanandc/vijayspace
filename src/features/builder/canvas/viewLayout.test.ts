import { describe, expect, it } from 'vitest';

import { sampleDefinition } from '../../../test/flowFixtures';
import { INSERT_SIZE, layoutDefinition } from '../model/layout';
import { updateStep } from '../../../domain/tree';
import { CARD_WIDTH, COLUMN_PITCH, ROW_STRETCH, reuseEdges, reuseNodes, toViewEdges, toViewNodes } from './viewLayout';

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

  it('keeps insert-point and End wrappers out of the tab order (the + is its own button)', () => {
    expect(view.get('ins:root:0')?.focusable).toBe(false);
    expect(view.get('end')?.focusable).toBe(false);
    expect(view.get('trigger')?.focusable).toBeUndefined();
  });
});

describe('reusing unchanged nodes', () => {
  it('keeps the objects of nodes an edit did not touch, so React Flow keeps their measurements', () => {
    const before = sampleDefinition();
    const first = layoutDefinition(before);
    const prevNodes = toViewNodes(first.nodes);
    const prevEdges = toViewEdges(first.edges);
    const after = updateStep(before, 'notify', (s) => ({ ...s, name: 'Tell the team' }));
    const second = layoutDefinition(after);
    const nodes = reuseNodes(prevNodes, toViewNodes(second.nodes));
    const edges = reuseEdges(prevEdges, toViewEdges(second.edges));
    const same = (id: string) => nodes.find((n) => n.id === id) === prevNodes.find((n) => n.id === id);
    expect(same('trigger')).toBe(true);
    expect(same('step:first')).toBe(true);
    expect(same('step:notify')).toBe(false);
    expect(edges.every((e, i) => e === prevEdges[i])).toBe(true);
  });

  it('numbers steps in reading order', () => {
    const numbers = layoutDefinition(sampleDefinition())
      .nodes.flatMap((n) => (n.data.kind === 'step' ? [`${n.data.step.key}:${n.data.number}`] : []));
    expect(numbers[0]).toBe('first:1');
    expect(numbers).toHaveLength(new Set(numbers).size);
  });
});
