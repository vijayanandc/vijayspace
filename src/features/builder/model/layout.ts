// Pure layout: definition tree → React Flow nodes and edges (vertical, branches as columns).

import type { Edge, Node } from '@xyflow/react';

import type { FlowDefinition, Step, TriggerNode } from '../../../api/types';
import { summarize } from './condition';
import { TRIGGER_ID } from './paths';
import { type InsertionPoint, type ListRef, ROOT, listRefId } from '../../../domain/tree';

export const COL_WIDTH = 300;
export const NODE_WIDTH = 240;
export const NODE_HEIGHT = 64;
export const INSERT_SIZE = 28;
export const GAP = 26;

export type TriggerNodeData = { kind: 'trigger'; trigger: TriggerNode };
/** `number`: the step's position in reading order (1-based), shown on its card. */
export type StepNodeData = { kind: 'step'; step: Step; list: ListRef; index: number; number: number };
export type InsertNodeData = { kind: 'insert'; point: InsertionPoint };
export type EndNodeData = { kind: 'end' };
export type BuilderNodeData = TriggerNodeData | StepNodeData | InsertNodeData | EndNodeData;

export type BuilderNode = Node<BuilderNodeData>;
export type BuilderEdge = Edge;

export function stepNodeId(key: string): string {
  return `step:${key}`;
}

export function insertNodeId(point: InsertionPoint): string {
  return `ins:${listRefId(point.list)}:${point.index}`;
}

/** Width of a list in columns. */
export function listWidth(steps: Step[]): number {
  return Math.max(1, ...steps.map(stepWidth));
}

function stepWidth(s: Step): number {
  if (s.type !== 'decision') return 1;
  return s.branches.reduce((sum, b) => sum + listWidth(b.steps), 0) + listWidth(s.otherwise);
}

interface Incoming {
  source: string;
  label?: string;
}

class Builder {
  nodes: BuilderNode[] = [];
  edges: BuilderEdge[] = [];
  /** Steps placed so far: steps are placed in reading order (pre-order, as `allSteps`). */
  steps = 0;

  node(id: string, type: string, x: number, y: number, data: BuilderNodeData): void {
    this.nodes.push({ id, type, position: { x, y }, data, draggable: false, selectable: data.kind !== 'insert' });
  }

  connect(from: Incoming[], target: string): void {
    for (const f of from) {
      this.edges.push({
        id: `e:${f.source}->${target}`,
        source: f.source,
        target,
        type: 'smoothstep',
        label: f.label,
        className: f.label ? 'edge-labelled' : undefined,
      });
    }
  }

  /** Lays out one list centred at `cx` starting at `y`; returns the bottom y and the exit node ids. */
  list(steps: Step[], ref: ListRef, cx: number, y: number, incoming: Incoming[]): { y: number; exits: string[] } {
    let prev = incoming;
    for (let i = 0; ; i += 1) {
      const point: InsertionPoint = { list: ref, index: i };
      const insertId = insertNodeId(point);
      this.node(insertId, 'insert', cx - INSERT_SIZE / 2, y, { kind: 'insert', point });
      this.connect(prev, insertId);
      y += INSERT_SIZE + GAP;
      prev = [{ source: insertId }];
      const step = steps[i];
      if (!step) break;
      const id = stepNodeId(step.key);
      this.node(id, 'step', cx - NODE_WIDTH / 2, y, { kind: 'step', step, list: ref, index: i, number: ++this.steps });
      this.connect(prev, id);
      y += NODE_HEIGHT + GAP;
      if (step.type === 'decision') {
        const columns: { ref: ListRef; steps: Step[]; label: string }[] = [
          ...step.branches.map((b) => ({
            ref: { kind: 'branch', decision: step.key, branch: b.key } as ListRef,
            steps: b.steps,
            label: b.name || summarize(b.condition),
          })),
          { ref: { kind: 'otherwise', decision: step.key }, steps: step.otherwise, label: 'Otherwise' },
        ];
        const total = columns.reduce((sum, c) => sum + listWidth(c.steps), 0);
        let left = cx - (total * COL_WIDTH) / 2;
        let bottom = y;
        const exits: string[] = [];
        for (const column of columns) {
          const w = listWidth(column.steps);
          const res = this.list(column.steps, column.ref, left + (w * COL_WIDTH) / 2, y, [{ source: id, label: column.label }]);
          left += w * COL_WIDTH;
          bottom = Math.max(bottom, res.y);
          exits.push(...res.exits);
        }
        y = bottom;
        prev = exits.map((source) => ({ source }));
      } else {
        prev = [{ source: id }];
      }
    }
    return { y, exits: prev.map((p) => p.source) };
  }
}

/** Computes the canvas for a definition. Node ids: `trigger`, `step:<key>`, `ins:<list>:<i>`, `end`. */
export function layoutDefinition(def: FlowDefinition): { nodes: BuilderNode[]; edges: BuilderEdge[] } {
  const b = new Builder();
  b.node(TRIGGER_ID, 'trigger', -NODE_WIDTH / 2, 0, { kind: 'trigger', trigger: def.trigger });
  const res = b.list(def.steps, ROOT, 0, NODE_HEIGHT + GAP, [{ source: TRIGGER_ID }]);
  b.node('end', 'end', -40, res.y, { kind: 'end' });
  b.connect(res.exits.map((source) => ({ source })), 'end');
  return { nodes: b.nodes, edges: b.edges };
}
