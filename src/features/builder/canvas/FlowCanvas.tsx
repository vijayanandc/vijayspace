// The flow canvas: React Flow rendering of the layout computed from the definition.

import { Background, Controls, ReactFlow, ReactFlowProvider, useReactFlow } from '@xyflow/react';
import { type KeyboardEvent, useEffect, useMemo, useRef } from 'react';

import { useBuilder } from '../BuilderContext';
import { layoutDefinition } from '../model/layout';
import { TRIGGER_ID } from '../model/paths';
import { nodeTypes } from './nodes';

function Canvas() {
  const { def, select, selected, deleteStep, setActivePoint } = useBuilder();
  const { nodes, edges } = useMemo(() => layoutDefinition(def), [def]);
  const { fitView } = useReactFlow();
  const stepCount = useRef(def.steps.length);

  useEffect(() => {
    // Re-fit when steps are added at the top level, so new nodes stay visible.
    if (def.steps.length !== stepCount.current) {
      stepCount.current = def.steps.length;
      window.requestAnimationFrame(() => void fitView({ padding: 0.2, duration: 200, maxZoom: 1 }));
    }
  }, [def.steps.length, fitView]);

  const onKeyDown = (e: KeyboardEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest('input, textarea, select, [contenteditable]')) return;
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
    <div className="canvas" onKeyDown={onKeyDown} tabIndex={-1}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable
        deleteKeyCode={null}
        fitView
        fitViewOptions={{ padding: 0.2, maxZoom: 1 }}
        minZoom={0.2}
        maxZoom={1.5}
        onNodeClick={(_, node) => {
          if (node.data.kind === 'trigger') select(TRIGGER_ID);
          if (node.data.kind === 'step') select(node.data.step.key);
        }}
        onPaneClick={() => {
          select(null);
          setActivePoint(null);
        }}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={20} />
        <Controls showInteractive={false} />
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
