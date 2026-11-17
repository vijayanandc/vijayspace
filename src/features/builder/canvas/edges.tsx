// Canvas connectors: rounded orthogonal lines, and branch lines with a pill label at the top of each column.

import { BaseEdge, EdgeLabelRenderer, type EdgeProps, getSmoothStepPath } from '@xyflow/react';

/** Lines that change column (branch exits merging back) bend just above the target, so merges line up. */
function path(p: EdgeProps, bendNearTarget: boolean): string {
  const [d] = getSmoothStepPath({
    sourceX: p.sourceX,
    sourceY: p.sourceY,
    sourcePosition: p.sourcePosition,
    targetX: p.targetX,
    targetY: p.targetY,
    targetPosition: p.targetPosition,
    borderRadius: 12,
    offset: 8,
    centerY: bendNearTarget && Math.abs(p.sourceX - p.targetX) > 1 ? p.targetY - 18 : undefined,
  });
  return d;
}

export function FlowEdge(props: EdgeProps) {
  return <BaseEdge id={props.id} path={path(props, true)} markerEnd={props.markerEnd} style={props.style} />;
}

export function BranchEdge(props: EdgeProps) {
  const label = typeof props.label === 'string' ? props.label : '';
  // The smooth-step line turns down into the column halfway between the decision and the column.
  const y = (props.sourceY + props.targetY) / 2;
  return (
    <>
      <BaseEdge id={props.id} path={path(props, false)} markerEnd={props.markerEnd} style={props.style} />
      {label && (
        <EdgeLabelRenderer>
          <div className="edge-pill" style={{ transform: `translate(-50%, -50%) translate(${props.targetX}px, ${y}px)` }} title={label}>
            {label}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}

export const edgeTypes = { flow: FlowEdge, branch: BranchEdge };
