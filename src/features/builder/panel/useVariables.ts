// The variables a step (or the trigger, with no key) can reference, rebuilt only when the definition, the apps or
// the trigger sample change.

import { useMemo } from 'react';

import { type VarNode, buildVariableTree } from '../../../domain/variables';
import { useBuilder } from '../BuilderContext';

export function useVariables(stepKey: string | undefined): VarNode[] {
  const { def, manifests, flow } = useBuilder();
  const sample = flow.trigger?.sample;
  return useMemo(() => buildVariableTree(def, stepKey, manifests, sample), [def, stepKey, manifests, sample]);
}
