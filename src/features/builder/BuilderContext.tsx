// Shared builder state. `BuilderContext` (everything, for panels and dialogs) changes with every edit;
// `BuilderViewContext` (what canvas cards and palette items need) does not change while a panel field is typed in.

import { type ReactNode, createContext, useContext } from 'react';

import type { ConnectorManifest, Flow, FlowDefinition, Issue } from '../../api/types';
import type { DragPayload, PaletteItem } from '../../domain/factories';
import type { InsertionPoint } from '../../domain/tree';

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
  /** The step being moved without dragging ("Move to…"): the next insertion point chosen places it. */
  moving: string | null;
  startMove: (key: string | null) => void;
  /** A viewer: the flow is shown, not edited (no palette, insert points, drag or delete). */
  readOnly: boolean;
  /** Saves pending changes now; resolves false when saving failed. */
  flush: () => Promise<boolean>;
  refetchFlow: () => void;
}

/** What canvas cards, insertion points and palette items read: stable while the definition is edited. */
export type BuilderViewCtx = Pick<
  BuilderCtx,
  | 'manifests'
  | 'selected'
  | 'select'
  | 'issues'
  | 'activePoint'
  | 'setActivePoint'
  | 'drop'
  | 'insertItem'
  | 'deleteStep'
  | 'readOnly'
  | 'moving'
  | 'startMove'
>;

export const BuilderContext = createContext<BuilderCtx | null>(null);
export const BuilderViewContext = createContext<BuilderViewCtx | null>(null);

export function useBuilderView(): BuilderViewCtx {
  const ctx = useContext(BuilderViewContext);
  if (!ctx) throw new Error('useBuilderView must be used inside the builder');
  return ctx;
}

/** The builder context when rendered inside the builder, else null (for components shared with other pages). */
export function useOptionalBuilder(): BuilderCtx | null {
  return useContext(BuilderContext);
}

export function useBuilder(): BuilderCtx {
  const ctx = useContext(BuilderContext);
  if (!ctx) throw new Error('useBuilder must be used inside the builder');
  return ctx;
}

/** For tests: both contexts from one value. */
export function TestBuilderProvider({ value, children }: { value: BuilderCtx; children: ReactNode }) {
  return (
    <BuilderContext.Provider value={value}>
      <BuilderViewContext.Provider value={value}>{children}</BuilderViewContext.Provider>
    </BuilderContext.Provider>
  );
}
