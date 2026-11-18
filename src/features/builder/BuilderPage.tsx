// The flow builder: top bar, then palette | canvas | side panel, with autosave and undo/redo.

import { useIsMutating, useQuery } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useReducer, useState } from 'react';
import { useParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { mk, qk, useConnectors } from '../../api/hooks';
import type { ConnectorManifest, Flow, FlowDefinition } from '../../api/types';
import { Button } from '../../components/Button';
import { ErrorBox, Loading } from '../../components/Empty';
import { MessageBar } from '../../components/MessageBar';
import { BuilderContext, type BuilderCtx } from './BuilderContext';
import { FlowCanvas } from './canvas/FlowCanvas';
import { IssuesPanel } from './IssuesPanel';
import { createStep, type DragPayload, type PaletteItem } from './model/factories';
import { historyReducer, initHistory } from './model/history';
import { issuesByNode } from './model/paths';
import { followSelection } from './model/selection';
import { type InsertionPoint, ROOT, deleteStep as removeStep, findStep, insertStep, moveStep } from './model/tree';
import { Palette } from './Palette';
import { SidePanel } from './panel/SidePanel';
import { Toolbar } from './Toolbar';
import { useAutosave } from './useAutosave';
import { can } from '../auth/permissions';
import { useRole } from '../auth/session';

export function BuilderPage() {
  const { flowId = '' } = useParams();
  return <BuilderLoader key={flowId} flowId={flowId} />;
}

/**
 * Opens the builder only on fresh data: the cached flow can predate the last autosave, so it waits for a
 * save still in flight from a previous visit and always refetches on mount. Once open, a failed background
 * refetch (listening for a sample, after publish) is reported without replacing the builder.
 */
function BuilderLoader({ flowId }: { flowId: string }) {
  const [opened, setOpened] = useState(false);
  const saving = useIsMutating({ mutationKey: mk.saveDraft(flowId) }) > 0;
  const flowQ = useQuery({
    queryKey: qk.flow(flowId),
    queryFn: () => endpoints.flow(flowId),
    enabled: opened || !saving,
    refetchOnMount: 'always',
  });
  const connectors = useConnectors();
  if (!opened && !saving && flowQ.isFetchedAfterMount && flowQ.data && !flowQ.error) setOpened(true);
  if (!opened || connectors.isLoading) {
    if (!opened && flowQ.error && !flowQ.isFetching && !saving) {
      return (
        <div className="page">
          <ErrorBox message={errorMessage(flowQ.error)} />
        </div>
      );
    }
    return <Loading label="Opening the builder…" />;
  }
  if (!flowQ.data) return null;
  return (
    <LoadedBuilder
      key={flowId}
      flow={flowQ.data}
      manifests={connectors.data ?? []}
      refetch={() => flowQ.refetch()}
      refreshError={flowQ.error ? errorMessage(flowQ.error) : null}
    />
  );
}

interface LoadedProps {
  flow: Flow;
  manifests: ConnectorManifest[];
  refetch: () => Promise<{ data?: Flow }>;
  /** A background refetch failed; the builder keeps working on the data it has. */
  refreshError: string | null;
}

function LoadedBuilder({ flow, manifests, refetch, refreshError }: LoadedProps) {
  const [history, dispatch] = useReducer(historyReducer, flow.draft, initHistory);
  const def = history.present;
  const autosave = useAutosave(flow.id, def, flow.draft_revision);
  const [selected, select] = useState<string | null>(null);
  // Undo/redo can remove the selected key (e.g. undoing a rename): follow it, or clear the selection.
  const [shownDef, setShownDef] = useState(def);
  if (shownDef !== def) {
    setShownDef(def);
    const kept = followSelection(shownDef, def, selected);
    if (kept !== selected) select(kept);
  }
  const [activePoint, setActivePoint] = useState<InsertionPoint | null>(null);
  const [showIssues, setShowIssues] = useState(false);
  const issues = useMemo(() => issuesByNode(def, autosave.issues), [def, autosave.issues]);

  // Viewers look at flows without changing them: edits are not applied (and so never saved).
  const readOnly = !can(useRole(), 'flow.edit');
  const apply = useCallback(
    (next: FlowDefinition, tag?: string) => {
      if (!readOnly) dispatch({ type: 'apply', def: next, tag });
    },
    [readOnly],
  );

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
          issues={autosave.issues}
          issuesOpen={showIssues}
          onToggleIssues={() => setShowIssues((v) => !v)}
          onIssues={(list) => {
            autosave.setIssues(list);
            setShowIssues(true);
          }}
          readOnly={readOnly}
        />
        {readOnly && (
          <MessageBar intent="info" role="status" className="builder-banner">
            View only: as a viewer you can look at this flow but not change, test or publish it.
          </MessageBar>
        )}
        {refreshError && (
          <MessageBar intent="warning" role="status" className="builder-banner" actions={<Button size="sm" onClick={() => void refetch()}>Try again</Button>}>
            Could not refresh this flow from the server ({refreshError}). You can keep editing.
          </MessageBar>
        )}
        {autosave.status === 'conflict' && (
          <MessageBar intent="warning" role="alert" className="builder-banner" actions={<Button size="sm" onClick={() => void reload()}>Reload the latest version</Button>}>
            This flow was changed somewhere else, so your latest edits were not saved.
          </MessageBar>
        )}
        <div className="builder-body">
          <Palette />
          <div className="builder-stage">
            <FlowCanvas />
            {showIssues && <IssuesPanel issues={autosave.issues} onClose={() => setShowIssues(false)} />}
          </div>
          <SidePanel />
        </div>
      </div>
    </BuilderContext.Provider>
  );
}
