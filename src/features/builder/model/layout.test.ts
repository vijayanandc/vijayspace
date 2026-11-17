import { describe, expect, it } from 'vitest';

import { action, sampleDefinition } from './fixtures';
import { COL_WIDTH, layoutDefinition, listWidth } from './layout';

describe('layout', () => {
  it('lays out an empty flow as trigger → insert → end', () => {
    const { nodes, edges } = layoutDefinition({ schema_version: 1, trigger: { type: 'manual' }, steps: [] });
    expect(nodes.map((n) => n.id)).toEqual(['trigger', 'ins:root:0', 'end']);
    expect(edges.map((e) => `${e.source}>${e.target}`)).toEqual(['trigger>ins:root:0', 'ins:root:0>end']);
  });

  it('places decision branches side by side and merges them', () => {
    const def = sampleDefinition();
    const { nodes, edges } = layoutDefinition(def);
    const byId = new Map(nodes.map((n) => [n.id, n]));
    const notify = byId.get('step:notify');
    const thank = byId.get('step:thank');
    expect(notify && thank).toBeTruthy();
    if (!notify || !thank) return;
    expect(notify.position.y).toBe(thank.position.y);
    expect(thank.position.x - notify.position.x).toBe(2 * COL_WIDTH); // big | small | otherwise
    // Each branch column ends in an insert node that connects to the next root insert point.
    const merges = edges.filter((e) => e.target === 'ins:root:2').map((e) => e.source).sort();
    expect(merges).toEqual(['ins:b:dec:big:1', 'ins:b:dec:small:0', 'ins:o:dec:1']);
    // Branch edges are labelled.
    expect(edges.find((e) => e.source === 'step:dec' && e.target === 'ins:o:dec:0')?.label).toBe('Otherwise');
    expect(edges.find((e) => e.source === 'step:dec' && e.target === 'ins:b:dec:big:0')?.label).toBe('Big');
  });

  it('computes list widths recursively', () => {
    expect(listWidth([])).toBe(1);
    expect(listWidth([action('a')])).toBe(1);
    expect(listWidth(sampleDefinition().steps)).toBe(3);
  });

  it('gives every node a unique id', () => {
    const { nodes } = layoutDefinition(sampleDefinition());
    expect(new Set(nodes.map((n) => n.id)).size).toBe(nodes.length);
  });
});
