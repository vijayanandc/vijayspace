// The flow builder: palette | canvas | side panel, with autosave and undo/redo.

import { useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useReducer, useState } from 'react';
import { useParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnectors } from '../../api/hooks';
import type { ConnectorManifest, Flow, FlowDefinition } from '../../api/types';
import { Button } from '../../components/Button';
import { ErrorBox, Loading } from '../../components/Empty';
import { BuilderContext, type BuilderCtx } from './BuilderContext';
import { FlowCanvas } from './canvas/FlowCanvas';
import { IssuesPanel } from './IssuesPanel';
import { createStep, type DragPayload, type PaletteItem } from './model/factories';
import { historyReducer, initHistory } from './model/history';
import { issuesByNode } from './model/paths';
import { type InsertionPoint, ROOT, deleteStep as removeStep, findStep, insertStep, moveStep } from './model/tree';
import { Palette } from './Palette';
import { SidePanel } from './panel/SidePanel';
import { Toolbar } from './Toolbar';
import { useAutosave } from './useAutosave';

export function BuilderPage() {
  const { flowId = '' } = useParams();
  const flowQ = useQuery({ queryKey: qk.flow(flowId), queryFn: () => endpoints.flow(flowId) });
  const connectors = useConnectors();
  if (flowQ.isLoading || connectors.isLoading) return <Loading />;
  if (flowQ.error) return <ErrorBox message={errorMessage(flowQ.error)} />;
  if (!flowQ.data) return null;
  return <LoadedBuilder key={flowId} flow={flowQ.data} manifests={connectors.data ?? []} refetch={() => flowQ.refetch()} />;
}

interface LoadedProps {
  flow: Flow;
  manifests: ConnectorManifest[];
  refetch: () => Promise<{ data?: Flow }>;
}

function LoadedBuilder({ flow, manifests, refetch }: LoadedProps) {
  const [history, dispatch] = useReducer(historyReducer, flow.draft, initHistory);
  const def = history.present;
  const autosave = useAutosave(flow.id, def, flow.draft_revision);
  const [selected, select] = useState<string | null>(null);
  const [activePoint, setActivePoint] = useState<InsertionPoint | null>(null);
  const [showIssues, setShowIssues] = useState(false);
  const issues = useMemo(() => issuesByNode(def, autosave.issues), [def, autosave.issues]);

  const apply = useCallback((next: FlowDefinition, tag?: string) => dispatch({ type: 'apply', def: next, tag }), []);

  const placeNew = useCallback(
    (point: InsertionPoint, item: PaletteItem) => {
      const step = createStep(def, item, manifests);
      apply(insertStep(def, point, step));
      select(step.key);
      setActivePoint(null);
    },
    [def, manifests, apply],
  );

  const ctx: BuilderCtx = {
    flowId: flow.id,
    flow,
    def,
    apply,
    manifests,
    selected,
    select,
    issues,
    activePoint,
    setActivePoint,
    drop: (point: InsertionPoint, payload: DragPayload) => {
      if (payload.kind === 'palette') placeNew(point, payload.item);
      else apply(moveStep(def, payload.key, point));
    },
    insertItem: (item: PaletteItem) => placeNew(activePoint ?? { list: ROOT, index: def.steps.length }, item),
    deleteStep: (key: string) => {
      const step = findStep(def, key);
      if (!step) return;
      const nested = step.type === 'decision' ? step.branches.reduce((n, b) => n + b.steps.length, step.otherwise.length) : 0;
      if (nested > 0 && !window.confirm(`Delete “${step.name ?? key}” and the ${nested} step(s) inside it?`)) return;
      apply(removeStep(def, key));
      if (selected === key) select(null);
    },
    flush: autosave.flush,
    refetchFlow: () => void refetch(),
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest('input, textarea, select')) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') {
        e.preventDefault();
        dispatch({ type: e.shiftKey ? 'redo' : 'undo' });
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'y') {
        e.preventDefault();
        dispatch({ type: 'redo' });
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const reload = async () => {
    const fresh = await refetch();
    if (fresh.data) {
      dispatch({ type: 'reset', def: fresh.data.draft });
      autosave.reset(fresh.data.draft, fresh.data.draft_revision);
    }
  };

  return (
    <BuilderContext.Provider value={ctx}>
      <div className="builder">
        <Toolbar
          saveStatus={autosave.status}
          saveError={autosave.error}
          canUndo={history.past.length > 0}
          canRedo={history.future.length > 0}
          onUndo={() => dispatch({ type: 'undo' })}
          onRedo={() => dispatch({ type: 'redo' })}
          issueCount={autosave.issues.length}
          onToggleIssues={() => setShowIssues((v) => !v)}
          onIssues={(list) => {
            autosave.setIssues(list);
            setShowIssues(true);
          }}
        />
        {autosave.status === 'conflict' && (
          <div className="banner banner-warning" role="alert">
            This flow was changed somewhere else, so your latest edits were not saved.
            <Button onClick={() => void reload()}>Reload the latest version</Button>
          </div>
        )}
        <div className="builder-body">
          <Palette />
          <FlowCanvas />
          <SidePanel />
        </div>
        {showIssues && <IssuesPanel issues={autosave.issues} onClose={() => setShowIssues(false)} />}
      </div>
    </BuilderContext.Provider>
  );
}
