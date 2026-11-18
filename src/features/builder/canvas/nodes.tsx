// Custom React Flow nodes: trigger and step cards, the "+" insertion point (drop target) and the End terminator.

import { Handle, type NodeProps, Position } from '@xyflow/react';
import { Plus, Trash2, Zap } from 'lucide-react';
import { type CSSProperties, type DragEvent, type KeyboardEvent, type ReactNode, useMemo, useState } from 'react';

import { AppIcon, tileColor } from '../../../components/AppIcon';
import { useBuilder } from '../BuilderContext';
import { isWeftDrag, readDrag, writeMoveDrag } from '../model/factories';
import { isScriptStep } from '../model/script';
import type { BuilderNode } from '../model/layout';
import { TRIGGER_ID } from '../model/paths';
import { allSteps, sameList } from '../model/tree';
import { type NodeLabel, stepLabel, triggerLabel } from './labels';

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

/** Card body shared by the trigger and step nodes. */
function CardBody({ label, number, children }: { label: NodeLabel; number: ReactNode; children?: ReactNode }) {
  return (
    <>
      <AppIcon app={label.icon} name={label.iconName} size={36} decorative />
      <div className="node-text">
        <div className="node-kicker">
          <span className="node-num">{number}</span>
          {label.kind}
        </div>
        <div className="node-title">{label.title}</div>
        <div className="node-subtitle">{label.subtitle}</div>
      </div>
      {children}
    </>
  );
}

function accent(icon: string): CSSProperties {
  return { '--accent': tileColor(icon) } as CSSProperties;
}

export function TriggerNodeView({ data }: NodeProps<BuilderNode>) {
  const { selected, manifests } = useBuilder();
  if (data.kind !== 'trigger') return null;
  const label = triggerLabel(data.trigger, manifests);
  return (
    <div className={`node node-trigger${selected === TRIGGER_ID ? ' node-selected' : ''}`} style={accent(label.icon)}>
      <CardBody label={label} number={<Zap size={10} strokeWidth={2.5} aria-hidden="true" />} />
      <IssueDot id={TRIGGER_ID} />
      <Handles />
    </div>
  );
}

export function StepNodeView({ data }: NodeProps<BuilderNode>) {
  const { selected, manifests, deleteStep, def } = useBuilder();
  const stepKey = data.kind === 'step' ? data.step.key : '';
  const number = useMemo(() => allSteps(def).findIndex((s) => s.key === stepKey) + 1, [def, stepKey]);
  if (data.kind !== 'step') return null;
  const { step } = data;
  const label = stepLabel(step, manifests);
  const onDragStart = (e: DragEvent) => writeMoveDrag(e.dataTransfer, step.key);
  return (
    <div
      className={`node node-step node-${step.type}${isScriptStep(step) ? ' node-script' : ''}${selected === step.key ? ' node-selected' : ''} nodrag nopan`}
      style={accent(label.icon)}
      draggable
      onDragStart={onDragStart}
      title="Drag onto a + to move"
    >
      <CardBody label={label} number={number}>
        <button
          type="button"
          className="node-delete"
          aria-label={`Delete ${label.title}`}
          title="Delete step"
          onClick={(e) => {
            e.stopPropagation();
            deleteStep(step.key);
          }}
        >
          <Trash2 size={14} aria-hidden="true" />
        </button>
      </CardBody>
      <IssueDot id={step.key} />
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
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== 'Enter' && e.key !== ' ') return;
    e.preventDefault();
    setActivePoint(active ? null : point);
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
      onKeyDown={onKeyDown}
      role="button"
      tabIndex={0}
      aria-label="Insert a step here"
      aria-pressed={active}
      title="Drop a step here, or click and then pick from the palette"
    >
      <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
      <Handles />
    </div>
  );
}

export function EndNodeView() {
  return (
    <div className="end-node">
      <span className="end-dot" aria-hidden="true" />
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
