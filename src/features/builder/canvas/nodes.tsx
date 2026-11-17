// Custom React Flow nodes: trigger, step, insertion point (+ drop target) and end.

import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type DragEvent, useState } from 'react';

import type { ConnectorManifest, Step, TriggerNode } from '../../../api/types';
import { useBuilder } from '../BuilderContext';
import { LOGIC_ITEMS, isWeftDrag, readDrag, writeMoveDrag } from '../model/factories';
import type { BuilderNode } from '../model/layout';
import { TRIGGER_ID } from '../model/paths';
import { sameList } from '../model/tree';

function Handles() {
  return (
    <>
      <Handle type="target" position={Position.Top} isConnectable={false} className="handle-hidden" />
      <Handle type="source" position={Position.Bottom} isConnectable={false} className="handle-hidden" />
    </>
  );
}

function IssueDot({ id }: { id: string }) {
  const { issues } = useBuilder();
  const list = issues.get(id) ?? [];
  const errors = list.filter((i) => i.severity === 'error').length;
  if (list.length === 0) return null;
  return (
    <span className={`node-issues ${errors ? 'node-issues-error' : 'node-issues-warning'}`} title={list.map((i) => i.message).join('\n')}>
      {list.length}
    </span>
  );
}

export function triggerLabel(t: TriggerNode, manifests: ConnectorManifest[]): { title: string; subtitle: string; icon: string } {
  switch (t.type) {
    case 'webhook':
      return { title: 'Webhook', subtitle: t.split_array ? 'Splits arrays into runs' : 'When a request arrives', icon: '⚡' };
    case 'schedule':
      return { title: 'Schedule', subtitle: t.schedule.kind === 'cron' ? t.schedule.expr : t.schedule.kind === 'interval' ? `Every ${t.schedule.minutes} min` : `Once at ${t.schedule.at}`, icon: '⏰' };
    case 'app': {
      const m = manifests.find((x) => x.key === t.connector);
      const spec = m?.triggers.find((x) => x.key === t.trigger);
      return { title: m ? `${m.name}` : 'App event', subtitle: spec?.name ?? 'Choose an app trigger', icon: m?.icon ?? '🔔' };
    }
    case 'manual':
      return { title: 'Manual', subtitle: 'Run now or via the API', icon: '▶' };
  }
}

export function stepLabel(s: Step, manifests: ConnectorManifest[]): { title: string; subtitle: string; icon: string } {
  if (s.type === 'action') {
    const m = manifests.find((x) => x.key === s.connector);
    const a = m?.actions.find((x) => x.key === s.action);
    return { title: s.name || a?.name || s.action, subtitle: `${m?.name ?? s.connector} · ${a?.name ?? s.action}`, icon: m?.icon ?? '⚙' };
  }
  const logic = LOGIC_ITEMS.find((l) => l.type === s.type);
  let subtitle = logic?.description ?? '';
  if (s.type === 'decision') subtitle = `${s.branches.length} branch${s.branches.length === 1 ? '' : 'es'} + otherwise`;
  if (s.type === 'delay') subtitle = s.until ? `Until ${s.until}` : formatSeconds(s.seconds ?? 0);
  if (s.type === 'set_variable') subtitle = s.assignments.map((a) => a.name).join(', ') || 'No variables';
  if (s.type === 'stop') subtitle = s.status === 'failed' ? 'Stop as failed' : 'Stop as succeeded';
  return { title: s.name || logic?.label || s.type, subtitle, icon: logic?.icon ?? '•' };
}

export function formatSeconds(seconds: number): string {
  if (seconds % 86_400 === 0 && seconds > 0) return `Wait ${seconds / 86_400} day(s)`;
  if (seconds % 3600 === 0 && seconds > 0) return `Wait ${seconds / 3600} hour(s)`;
  if (seconds % 60 === 0 && seconds > 0) return `Wait ${seconds / 60} minute(s)`;
  return `Wait ${seconds} s`;
}

export function TriggerNodeView({ data }: NodeProps<BuilderNode>) {
  const { selected, manifests } = useBuilder();
  if (data.kind !== 'trigger') return null;
  const label = triggerLabel(data.trigger, manifests);
  return (
    <div className={`node node-trigger${selected === TRIGGER_ID ? ' node-selected' : ''}`}>
      <span className="node-icon" aria-hidden="true">{label.icon}</span>
      <div className="node-text">
        <div className="node-title">{label.title}</div>
        <div className="node-subtitle">{label.subtitle}</div>
      </div>
      <IssueDot id={TRIGGER_ID} />
      <Handles />
    </div>
  );
}

export function StepNodeView({ data }: NodeProps<BuilderNode>) {
  const { selected, manifests, deleteStep } = useBuilder();
  if (data.kind !== 'step') return null;
  const { step } = data;
  const label = stepLabel(step, manifests);
  const onDragStart = (e: DragEvent) => writeMoveDrag(e.dataTransfer, step.key);
  return (
    <div
      className={`node node-step node-${step.type}${selected === step.key ? ' node-selected' : ''} nodrag nopan`}
      draggable
      onDragStart={onDragStart}
      title="Drag onto a + to move"
    >
      <span className="node-icon" aria-hidden="true">{label.icon}</span>
      <div className="node-text">
        <div className="node-title">{label.title}</div>
        <div className="node-subtitle">{label.subtitle}</div>
      </div>
      <IssueDot id={step.key} />
      <button
        type="button"
        className="node-delete"
        aria-label={`Delete ${label.title}`}
        onClick={(e) => {
          e.stopPropagation();
          deleteStep(step.key);
        }}
      >
        ×
      </button>
      <Handles />
    </div>
  );
}

export function InsertNodeView({ data }: NodeProps<BuilderNode>) {
  const { drop, activePoint, setActivePoint } = useBuilder();
  const [over, setOver] = useState(false);
  if (data.kind !== 'insert') return null;
  const { point } = data;
  const active = activePoint !== null && sameList(activePoint.list, point.list) && activePoint.index === point.index;
  const onDragOver = (e: DragEvent) => {
    if (!isWeftDrag(e.dataTransfer)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = e.dataTransfer.types.includes('application/x-weft-step') ? 'move' : 'copy';
    setOver(true);
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    const payload = readDrag(e.dataTransfer);
    if (payload) drop(point, payload);
  };
  return (
    <div
      className={`insert-node nodrag nopan${over ? ' insert-over' : ''}${active ? ' insert-active' : ''}`}
      onDragOver={onDragOver}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
      onClick={(e) => {
        e.stopPropagation();
        setActivePoint(active ? null : point);
      }}
      role="button"
      tabIndex={0}
      aria-label="Insert a step here"
      title="Drop a step here, or click and then pick from the palette"
    >
      +
      <Handles />
    </div>
  );
}

export function EndNodeView() {
  return (
    <div className="end-node">
      End
      <Handle type="target" position={Position.Top} isConnectable={false} className="handle-hidden" />
    </div>
  );
}

export const nodeTypes = {
  trigger: TriggerNodeView,
  step: StepNodeView,
  insert: InsertNodeView,
  end: EndNodeView,
};
