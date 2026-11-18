// Presentation-only geometry on top of model/layout: wider cards, roomier columns and rows, styled edges.
// The model positions nodes for 240px cards in 300px columns; this keeps every node centred on the same
// column while spreading columns and rows out, so the tree shape never changes.

import { type Edge, MarkerType } from '@xyflow/react';

import { type BuilderEdge, type BuilderNode, COL_WIDTH, INSERT_SIZE, NODE_WIDTH } from '../model/layout';

/** Rendered width of trigger and step cards. */
export const CARD_WIDTH = 296;
/** Distance between decision branch columns on screen. */
export const COLUMN_PITCH = 344;
/** Vertical stretch applied to model y positions (cards keep their 64px height). */
export const ROW_STRETCH = 1.25;
/** Width the model assumes for the End terminator (it places it at x = -40). */
const END_WIDTH = 80;

export const EDGE_COLOR = '#8a8a8a';

function widths(type: string | undefined): { model: number; view: number } {
  if (type === 'insert') return { model: INSERT_SIZE, view: INSERT_SIZE };
  if (type === 'end') return { model: END_WIDTH, view: END_WIDTH };
  return { model: NODE_WIDTH, view: CARD_WIDTH };
}

/** Re-positions model nodes for the wider cards: centres scale by the column pitch, y by the row stretch. */
export function toViewNodes(nodes: BuilderNode[]): BuilderNode[] {
  const sx = COLUMN_PITCH / COL_WIDTH;
  return nodes.map((n) => {
    const w = widths(n.type);
    const centre = (n.position.x + w.model / 2) * sx;
    // Insert points carry their own focusable button and the End is inert: their wrappers stay out of the tab order.
    const focusable = n.type === 'insert' || n.type === 'end' ? false : undefined;
    return { ...n, focusable, position: { x: centre - w.view / 2, y: n.position.y * ROW_STRETCH } };
  });
}

function sameData(a: BuilderNode['data'], b: BuilderNode['data']): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === 'step' && b.kind === 'step') {
    return a.step === b.step && a.index === b.index && a.number === b.number && JSON.stringify(a.list) === JSON.stringify(b.list);
  }
  if (a.kind === 'trigger' && b.kind === 'trigger') return a.trigger === b.trigger;
  if (a.kind === 'insert' && b.kind === 'insert') return JSON.stringify(a.point) === JSON.stringify(b.point);
  return true;
}

/**
 * Keeps the previous object for every node whose position and data did not change. React Flow keeps a node's
 * measurements only when it gets the same object back; a new object is hidden, measured again and its edges redrawn.
 */
export function reuseNodes(prev: BuilderNode[], next: BuilderNode[]): BuilderNode[] {
  const old = new Map(prev.map((n) => [n.id, n]));
  return next.map((n) => {
    const p = old.get(n.id);
    const same =
      p !== undefined &&
      p.type === n.type &&
      p.position.x === n.position.x &&
      p.position.y === n.position.y &&
      p.focusable === n.focusable &&
      sameData(p.data, n.data);
    return same ? p : n;
  });
}

/** Keeps the previous object for every edge that did not change (edges are small: compared by value). */
export function reuseEdges(prev: Edge[], next: Edge[]): Edge[] {
  const old = new Map(prev.map((e) => [e.id, e]));
  return next.map((e) => {
    const p = old.get(e.id);
    return p && JSON.stringify(p) === JSON.stringify(e) ? p : e;
  });
}

/** Branch edges get the pill label edge; edges into cards and the End get an arrowhead. */
export function toViewEdges(edges: BuilderEdge[]): Edge[] {
  return edges.map((e) => {
    const intoCard = !e.target.startsWith('ins:');
    return {
      ...e,
      type: e.label ? 'branch' : 'flow',
      markerEnd: intoCard ? { type: MarkerType.ArrowClosed, width: 16, height: 16, color: EDGE_COLOR } : undefined,
      style: { stroke: EDGE_COLOR, strokeWidth: 1.5 },
    };
  });
}
