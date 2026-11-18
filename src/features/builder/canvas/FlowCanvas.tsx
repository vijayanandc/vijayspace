// The flow canvas: React Flow rendering of the layout computed from the definition (dotted background,
// zoom controls, drop targets highlighted while a palette item or step is being dragged).

// React Flow's own styles load with the builder; the overrides in styles/canvas.css are more specific.
import '@xyflow/react/dist/style.css';
import { Background, BackgroundVariant, Controls, type Edge, Panel, ReactFlow, ReactFlowProvider, useReactFlow } from '@xyflow/react';
import { MousePointerClick } from 'lucide-react';
import { type KeyboardEvent, type RefObject, useEffect, useMemo, useRef } from 'react';

import { useBuilder } from '../BuilderContext';
import { isWeftDrag } from '../../../domain/factories';
import { type BuilderNode, layoutDefinition, stepNodeId } from '../model/layout';
import { TRIGGER_ID } from '../model/paths';
import { edgeTypes } from './edges';
import { nodeTypes } from './nodes';
import { CARD_WIDTH, reuseEdges, reuseNodes, toViewEdges, toViewNodes } from './viewLayout';

/** Adds `canvas-dragging` to the canvas while a Weft drag is in progress (no re-render needed). */
function useDragHighlight(ref: RefObject<HTMLDivElement>) {
  useEffect(() => {
    const on = (e: DragEvent) => {
      if (e.dataTransfer && isWeftDrag(e.dataTransfer)) ref.current?.classList.add('canvas-dragging');
    };
    const off = () => ref.current?.classList.remove('canvas-dragging');
    document.addEventListener('dragstart', on);
    document.addEventListener('dragend', off);
    document.addEventListener('drop', off);
    return () => {
      document.removeEventListener('dragstart', on);
      document.removeEventListener('dragend', off);
      document.removeEventListener('drop', off);
    };
  }, [ref]);
}

/** Pans (without zooming) so the selected card is not hidden behind the side panel or the canvas edges. */
function useKeepSelectionVisible(selected: string | null, ref: RefObject<HTMLDivElement>) {
  const { getNode, getViewport, setViewport } = useReactFlow();
  useEffect(() => {
    if (!selected) return undefined;
    const timer = window.setTimeout(() => {
      const node = getNode(selected === TRIGGER_ID ? TRIGGER_ID : stepNodeId(selected));
      const el = ref.current;
      if (!node || !el) return;
      const { x, y, zoom } = getViewport();
      const left = node.position.x * zoom + x;
      const top = node.position.y * zoom + y;
      const right = left + (node.measured?.width ?? CARD_WIDTH) * zoom;
      const bottom = top + (node.measured?.height ?? 64) * zoom;
      const margin = 32;
      let dx = 0;
      let dy = 0;
      if (right > el.clientWidth - margin) dx = el.clientWidth - margin - right;
      if (left + dx < margin) dx = margin - left;
      if (bottom > el.clientHeight - margin) dy = el.clientHeight - margin - bottom;
      if (top + dy < margin) dy = margin - top;
      if (dx !== 0 || dy !== 0) void setViewport({ x: x + dx, y: y + dy, zoom }, { duration: 200 });
    }, 60);
    return () => window.clearTimeout(timer);
  }, [selected, ref, getNode, getViewport, setViewport]);
}

const FIT_VIEW = { padding: 0.2, maxZoom: 1 };
const PRO_OPTIONS = { hideAttribution: true };

function Canvas() {
  const { def, select, selected, deleteStep, setActivePoint } = useBuilder();
  // Unchanged nodes and edges keep their objects (and so their measurements) across edits.
  const previous = useRef<{ nodes: BuilderNode[]; edges: Edge[] }>({ nodes: [], edges: [] });
  const { nodes, edges } = useMemo(() => {
    const model = layoutDefinition(def);
    const next = {
      nodes: reuseNodes(previous.current.nodes, toViewNodes(model.nodes)),
      edges: reuseEdges(previous.current.edges, toViewEdges(model.edges)),
    };
    previous.current = next;
    return next;
  }, [def]);
  const { fitView } = useReactFlow();
  const nodeCount = useRef(nodes.length);
  const ref = useRef<HTMLDivElement>(null);
  useDragHighlight(ref);
  useKeepSelectionVisible(selected, ref);

  useEffect(() => {
    // Re-fit when steps are added or removed anywhere (including inside branches),
    // so new nodes stay visible.
    if (nodes.length === nodeCount.current) return undefined;
    nodeCount.current = nodes.length;
    // Wait until React Flow has measured the new nodes; fitting earlier ignores them.
    const timer = window.setTimeout(() => void fitView({ padding: 0.2, duration: 200, maxZoom: 1 }), 80);
    return () => window.clearTimeout(timer);
  }, [nodes.length, fitView]);

  /** Opens the trigger or step panel for a card (click, or Enter/Space on the focused card). */
  const openNode = (node: BuilderNode) => {
    if (node.data.kind === 'trigger') select(TRIGGER_ID);
    if (node.data.kind === 'step') select(node.data.step.key);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('input, textarea, select, [contenteditable]')) return;
    if ((e.key === 'Enter' || e.key === ' ') && target.classList.contains('react-flow__node')) {
      // React Flow focuses the card wrapper (role="button") but only selects it internally on these keys.
      const node = nodes.find((n) => n.id === target.dataset.id);
      if (node) {
        e.preventDefault();
        openNode(node);
      }
      return;
    }
    if ((e.key === 'Delete' || e.key === 'Backspace') && selected && selected !== TRIGGER_ID) {
      e.preventDefault();
      deleteStep(selected);
    }
    if (e.key === 'Escape') {
      select(null);
      setActivePoint(null);
    }
  };

  return (
    <div className="canvas" ref={ref} onKeyDown={onKeyDown} tabIndex={-1}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable
        deleteKeyCode={null}
        fitView
        fitViewOptions={FIT_VIEW}
        minZoom={0.2}
        maxZoom={1.5}
        onNodeClick={(_, node) => openNode(node)}
        onPaneClick={() => {
          select(null);
          setActivePoint(null);
        }}
        proOptions={PRO_OPTIONS}
      >
        <Background variant={BackgroundVariant.Dots} gap={18} size={1.4} color="#c2c2c2" />
        {def.steps.length === 0 && (
          <Panel position="top-center" className="canvas-hint">
            <MousePointerClick size={16} aria-hidden="true" />
            Drag an app action or a logic step onto the <strong>+</strong> to add your first step.
          </Panel>
        )}
        <Controls showInteractive={false} position="bottom-right" />
      </ReactFlow>
    </div>
  );
}

export function FlowCanvas() {
  return (
    <ReactFlowProvider>
      <Canvas />
    </ReactFlowProvider>
  );
}
