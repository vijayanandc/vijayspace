// Debounced draft autosave with optimistic concurrency (revision) and conflict detection.

import { useCallback, useEffect, useRef, useState } from 'react';

import { ApiError, errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import type { FlowDefinition, Issue } from '../../api/types';

export type SaveStatus = 'saved' | 'pending' | 'saving' | 'error' | 'conflict';

const DELAY_MS = 1500;

export function useAutosave(flowId: string, def: FlowDefinition, initialRevision: number) {
  const [status, setStatus] = useState<SaveStatus>('saved');
  const [issues, setIssues] = useState<Issue[]>([]);
  const [error, setError] = useState<string | null>(null);
  const revision = useRef(initialRevision);
  const savedJson = useRef(JSON.stringify(def));
  const defRef = useRef(def);
  const timer = useRef<number | undefined>(undefined);
  const inFlight = useRef<Promise<boolean> | null>(null);
  const conflict = useRef(false);
  defRef.current = def;

  const save = useCallback(async (): Promise<boolean> => {
    if (conflict.current) return false;
    const current = defRef.current;
    const json = JSON.stringify(current);
    if (json === savedJson.current) {
      setStatus('saved');
      return true;
    }
    setStatus('saving');
    try {
      const res = await endpoints.saveDraft(flowId, current, revision.current);
      revision.current = res.revision;
      savedJson.current = json;
      setIssues(res.issues);
      setError(null);
      setStatus(JSON.stringify(defRef.current) === json ? 'saved' : 'pending');
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
  }, [flowId]);

  const run = useCallback((): Promise<boolean> => {
    const p = (inFlight.current ?? Promise.resolve(true)).then(() => save());
    inFlight.current = p.finally(() => {
      if (inFlight.current === p) inFlight.current = null;
    });
    return p;
  }, [save]);

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
    if (JSON.stringify(def) === savedJson.current) {
      // e.g. undo back to the saved state: nothing to save.
      window.clearTimeout(timer.current);
      if (!conflict.current) setStatus((s) => (s === 'pending' ? 'saved' : s));
      return undefined;
    }
    if (!conflict.current) setStatus('pending');
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void run(), DELAY_MS);
    return () => window.clearTimeout(timer.current);
  }, [def, run]);

  const flush = useCallback(async (): Promise<boolean> => {
    window.clearTimeout(timer.current);
    if (inFlight.current) await inFlight.current;
    return run();
  }, [run]);

  /** After reloading from the server (e.g. resolving a conflict). */
  const reset = useCallback((next: FlowDefinition, nextRevision: number) => {
    window.clearTimeout(timer.current);
    conflict.current = false;
    revision.current = nextRevision;
    savedJson.current = JSON.stringify(next);
    setStatus('saved');
    setError(null);
  }, []);

  return { status, issues, setIssues, error, flush, reset };
}
