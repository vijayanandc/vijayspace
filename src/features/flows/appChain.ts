// The app-icon chain of a flow: trigger app first, then each action app once, in step order.

import type { FlowDefinition, TriggerNode } from '../../api/types';
import { allSteps } from '../builder/model/tree';

/** Icon key of a trigger: the connector for app triggers, else `trigger:<type>`. */
export function triggerIconKey(trigger: TriggerNode): string {
  if (trigger.type === 'app' && trigger.connector) return trigger.connector;
  return `trigger:${trigger.type}`;
}

/** Icon key from a list row's `trigger_type` (used until the definition is known). */
export function triggerTypeIconKey(triggerType: string): string {
  return `trigger:${triggerType}`;
}

/** Deduplicated icon keys: the trigger, then action connectors depth-first. Logic steps are left out. */
export function appChain(def: FlowDefinition): string[] {
  const chain = [triggerIconKey(def.trigger)];
  for (const step of allSteps(def)) {
    if (step.type === 'action' && step.connector && !chain.includes(step.connector)) chain.push(step.connector);
  }
  return chain;
}

/** The first `max` keys, and how many more are hidden behind "+N". */
export function splitChain(chain: string[], max = 4): { shown: string[]; extra: number } {
  if (chain.length <= max) return { shown: chain, extra: 0 };
  return { shown: chain.slice(0, max - 1), extra: chain.length - (max - 1) };
}
