// Debounced draft autosave with optimistic concurrency (revision) and conflict detection.
// Saves run as TanStack mutations (`mk.saveDraft`) so a reopened builder can wait for one still in flight,
// and each success is written into the cached flow, so reopening never starts from a pre-save draft.

import { type QueryClient, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';

import { ApiError, errorMessage, getCurrentOrg } from '../../api/client';
import { useToast } from '../../components/Toast';
import { endpoints } from '../../api/endpoints';
import { mk, qk } from '../../api/hooks';
import type { Flow, FlowDefinition, Issue, SaveDraftResult } from '../../api/types';

export type SaveStatus = 'saved' | 'pending' | 'saving' | 'error' | 'conflict';

const DELAY_MS = 1500;

interface DraftSave {
  definition: FlowDefinition;
  revision: number;
}

/**
 * Puts a saved draft into the flow cache with what the save returned, so reopening never starts from a pre-save
 * draft. The flow is downloaded again only when the trigger type changed (its URL and registration come from the
 * server); the flows list is only marked stale.
 */
async function adoptSavedDraft(qc: QueryClient, flowId: string, draft: FlowDefinition, res: SaveDraftResult): Promise<void> {
  // A refetch started before this save would bring back the old draft: drop it.
  await qc.cancelQueries({ queryKey: qk.flow(flowId) });
  const before = qc.getQueryData<Flow>(qk.flow(flowId));
  qc.setQueryData<Flow>(qk.flow(flowId), (old) =>
    old ? { ...old, draft, draft_revision: res.revision, has_unpublished_changes: res.has_unpublished_changes } : old,
  );
  if (!before || before.draft.trigger.type !== draft.trigger.type) void qc.invalidateQueries({ queryKey: qk.flow(flowId) });
  void qc.invalidateQueries({ queryKey: qk.flows });
}

export function useAutosave(flowId: string, def: FlowDefinition, initialRevision: number) {
  const qc = useQueryClient();
  // The flow belongs to the org the builder opened in: a save that runs as the page closes for an org switch
  // must not land in (or 404 from) the next org.
  const [org] = useState(() => getCurrentOrg() ?? undefined);
  const { mutateAsync } = useMutation({
    mutationKey: mk.saveDraft(flowId),
    mutationFn: (v: DraftSave) => endpoints.saveDraft(flowId, v.definition, v.revision, org),
    onSuccess: (res, v) => adoptSavedDraft(qc, flowId, v.definition, res),
  });
  const [status, setStatus] = useState<SaveStatus>('saved');
  const [issues, setIssues] = useState<Issue[]>([]);
  const [error, setError] = useState<string | null>(null);
  const revision = useRef(initialRevision);
  // The last saved definition. Edits create new objects and undo returns the stored ones, so identity tells
  // whether anything is unsaved without serialising the draft on every keystroke.
  const savedDef = useRef(def);
  const defRef = useRef(def);
  const timer = useRef<number | undefined>(undefined);
  const inFlight = useRef<Promise<boolean> | null>(null);
  const conflict = useRef(false);
  const runRef = useRef<() => Promise<boolean>>(() => Promise.resolve(true));
  // The save on leaving runs after the builder is gone: only a toast can report its failure.
  const toast = useToast();
  const toastRef = useRef(toast);
  toastRef.current = toast;
  defRef.current = def;

  const schedule = useCallback(() => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void runRef.current(), DELAY_MS);
  }, []);

  const save = useCallback(async (): Promise<boolean> => {
    if (conflict.current) return false;
    const current = defRef.current;
    if (current === savedDef.current) {
      setStatus('saved');
      return true;
    }
    setStatus('saving');
    try {
      const res = await mutateAsync({ definition: current, revision: revision.current });
      revision.current = res.revision;
      savedDef.current = current;
      setIssues(res.issues);
      setError(null);
      if (defRef.current === current) {
        setStatus('saved');
      } else {
        // Edited, or undone back to the old state, while the request ran: save the current state too.
        setStatus('pending');
        schedule();
      }
      return true;
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        conflict.current = true;
        setStatus('conflict');
      } else {
        setStatus('error');
        setError(errorMessage(e));
      }
      return false;
    }
  }, [mutateAsync, schedule]);

  const run = useCallback((): Promise<boolean> => {
    const p = (inFlight.current ?? Promise.resolve(true)).then(() => save());
    // Resolves after the in-flight marker is cleared, so a caller that leaves right after a flush
    // (e.g. the OAuth redirect) is not asked "leave with unsaved changes?".
    const tracked: Promise<boolean> = p.finally(() => {
      if (inFlight.current === tracked) inFlight.current = null;
    });
    inFlight.current = tracked;
    return tracked;
  }, [save]);
  runRef.current = run;

  const flush = useCallback(async (): Promise<boolean> => {
    window.clearTimeout(timer.current);
    if (inFlight.current) await inFlight.current;
    window.clearTimeout(timer.current);
    return run();
  }, [run]);

  useEffect(() => {
    // Seed issues from the server on open: a draft (e.g. from a recipe) can be invalid before
    // anyone edits it, and the panel must not claim "no issues" until it has asked.
    let live = true;
    endpoints
      .validate(flowId)
      .then((res) => {
        if (live) setIssues(res.issues);
      })
      .catch(() => {
        /* the next save or publish reports issues */
      });
    return () => {
      live = false;
    };
  }, [flowId]);

  useEffect(() => {
    if (def === savedDef.current) {
      // e.g. undo back to the saved state: nothing to save.
      window.clearTimeout(timer.current);
      if (!conflict.current) setStatus((s) => (s === 'pending' ? 'saved' : s));
      return undefined;
    }
    if (!conflict.current) setStatus('pending');
    schedule();
    return () => window.clearTimeout(timer.current);
  }, [def, schedule]);

  useEffect(() => {
    // Leaving the builder (back link, another flow) saves what the debounce has not saved yet;
    // closing or reloading the tab asks first while anything is unsaved.
    const unsaved = () => inFlight.current !== null || defRef.current !== savedDef.current;
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!unsaved()) return;
      void runRef.current();
      e.preventDefault();
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => {
      window.removeEventListener('beforeunload', onBeforeUnload);
      window.clearTimeout(timer.current);
      if (unsaved()) {
        void runRef.current().then((saved) => {
          if (!saved) toastRef.current.show('error', 'Your last changes to this flow could not be saved. Open the flow again to check it.');
        });
      }
    };
  }, []);

  /** After reloading from the server (e.g. resolving a conflict). */
  const reset = useCallback((next: FlowDefinition, nextRevision: number) => {
    window.clearTimeout(timer.current);
    conflict.current = false;
    revision.current = nextRevision;
    savedDef.current = next;
    setStatus('saved');
    setError(null);
  }, []);

  return { status, issues, setIssues, error, flush, reset };
}
