// Shared builder state for the canvas nodes and side panels.

import { createContext, useContext } from 'react';

import type { ConnectorManifest, Flow, FlowDefinition, Issue } from '../../api/types';
import type { DragPayload, PaletteItem } from './model/factories';
import type { InsertionPoint } from './model/tree';

export interface BuilderCtx {
  flowId: string;
  flow: Flow;
  def: FlowDefinition;
  /** Applies a new definition (undoable). Edits with the same `tag` coalesce into one undo step. */
  apply: (def: FlowDefinition, tag?: string) => void;
  manifests: ConnectorManifest[];
  /** `trigger`, a step key, or null. */
  selected: string | null;
  select: (id: string | null) => void;
  issues: Map<string, Issue[]>;
  activePoint: InsertionPoint | null;
  setActivePoint: (p: InsertionPoint | null) => void;
  drop: (point: InsertionPoint, payload: DragPayload) => void;
  insertItem: (item: PaletteItem) => void;
  deleteStep: (key: string) => void;
  /** Saves pending changes now; resolves false when saving failed. */
  flush: () => Promise<boolean>;
  refetchFlow: () => void;
}

export const BuilderContext = createContext<BuilderCtx | null>(null);

/** The builder context when rendered inside the builder, else null (for components shared with other pages). */
export function useOptionalBuilder(): BuilderCtx | null {
  return useContext(BuilderContext);
}

export function useBuilder(): BuilderCtx {
  const ctx = useContext(BuilderContext);
  if (!ctx) throw new Error('useBuilder must be used inside the builder');
  return ctx;
}
