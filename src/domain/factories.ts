// Creating new steps, triggers and starter definitions.

import type {
  ConnectorManifest,
  Condition,
  FlowDefinition,
  JsonObject,
  Step,
  TriggerNode,
  TriggerType,
} from '../api/types';
import { uniqueKey } from './keys';
import { SCRIPT_ACTION, SCRIPT_CONNECTOR, starterScriptInput } from './script';

/** Logic palette items: the flow-control steps, plus custom functions (an action of `core.script`). */
export type LogicType = 'decision' | 'delay' | 'set_variable' | 'stop' | 'custom_function';

/** Something that can be dragged from the palette. */
export type PaletteItem =
  | { kind: 'logic'; type: LogicType }
  | { kind: 'action'; connector: string; action: string };

export const LOGIC_ITEMS: { type: LogicType; label: string; icon: string; description: string; group: 'Flow control' | 'Code' }[] = [
  { type: 'decision', label: 'Decision', icon: '◇', description: 'Branch on conditions', group: 'Flow control' },
  { type: 'delay', label: 'Delay', icon: '⏱', description: 'Wait for a time or until a date', group: 'Flow control' },
  { type: 'set_variable', label: 'Set variable', icon: '𝑥', description: 'Store values for later steps', group: 'Flow control' },
  { type: 'stop', label: 'Stop', icon: '■', description: 'End the flow', group: 'Flow control' },
  { type: 'custom_function', label: 'Custom function', icon: 'ƒ', description: 'Write a Rhai script for custom logic', group: 'Code' },
];

export function defaultCondition(): Condition {
  return { all: [{ left: '', op: 'eq', right: '' }] };
}

export function createStep(def: FlowDefinition, item: PaletteItem, manifests: ConnectorManifest[] = []): Step {
  if (item.kind === 'logic') {
    switch (item.type) {
      case 'decision':
        return {
          key: uniqueKey(def, 'decision'),
          name: 'Decision',
          type: 'decision',
          branches: [{ key: 'branch_1', name: 'Branch 1', condition: defaultCondition(), steps: [] }],
          otherwise: [],
        };
      case 'delay':
        return { key: uniqueKey(def, 'delay'), name: 'Delay', type: 'delay', seconds: 3600 };
      case 'set_variable':
        return { key: uniqueKey(def, 'set_variable'), name: 'Set variable', type: 'set_variable', assignments: [{ name: 'value', value: '' }] };
      case 'stop':
        return { key: uniqueKey(def, 'stop'), name: 'Stop', type: 'stop', status: 'succeeded' };
      case 'custom_function':
        return {
          key: uniqueKey(def, 'custom_function'),
          name: 'Custom function',
          type: 'action',
          connector: SCRIPT_CONNECTOR,
          action: SCRIPT_ACTION,
          connection_id: null,
          input: starterScriptInput(),
        };
    }
  }
  const manifest = manifests.find((m) => m.key === item.connector);
  const spec = manifest?.actions.find((a) => a.key === item.action);
  const input: JsonObject = {};
  for (const f of spec?.fields ?? []) {
    if (f.default !== undefined && f.default !== null) input[f.key] = f.default;
  }
  return {
    key: uniqueKey(def, item.action),
    name: spec?.name ?? item.action,
    type: 'action',
    connector: item.connector,
    action: item.action,
    connection_id: null,
    input,
  };
}

export function defaultTrigger(type: TriggerType): TriggerNode {
  switch (type) {
    case 'webhook':
      return { type: 'webhook', split_array: false };
    case 'schedule':
      return { type: 'schedule', schedule: { kind: 'cron', expr: '0 9 * * *', timezone: 'UTC' } };
    case 'app':
      return { type: 'app', connector: '', trigger: '', connection_id: null, input: {} };
    case 'manual':
      return { type: 'manual' };
  }
}

export function starterDefinition(type: TriggerType): FlowDefinition {
  return { schema_version: 1, trigger: defaultTrigger(type), steps: [] };
}

const MIME_PALETTE = 'application/x-weft-palette';
const MIME_MOVE = 'application/x-weft-step';

export function writePaletteDrag(dt: DataTransfer, item: PaletteItem): void {
  dt.setData(MIME_PALETTE, JSON.stringify(item));
  dt.effectAllowed = 'copy';
}

export function writeMoveDrag(dt: DataTransfer, key: string): void {
  dt.setData(MIME_MOVE, key);
  dt.effectAllowed = 'move';
}

export type DragPayload = { kind: 'palette'; item: PaletteItem } | { kind: 'move'; key: string };

export function readDrag(dt: DataTransfer): DragPayload | undefined {
  const moving = dt.getData(MIME_MOVE);
  if (moving) return { kind: 'move', key: moving };
  const raw = dt.getData(MIME_PALETTE);
  if (!raw) return undefined;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) return undefined;
    const p = parsed as Record<string, unknown>;
    if (p.kind === 'logic' && typeof p.type === 'string' && LOGIC_ITEMS.some((l) => l.type === p.type)) {
      return { kind: 'palette', item: { kind: 'logic', type: p.type as LogicType } };
    }
    if (p.kind === 'action' && typeof p.connector === 'string' && typeof p.action === 'string') {
      return { kind: 'palette', item: { kind: 'action', connector: p.connector, action: p.action } };
    }
  } catch {
    return undefined;
  }
  return undefined;
}

export function isWeftDrag(dt: DataTransfer): boolean {
  return dt.types.includes(MIME_PALETTE) || dt.types.includes(MIME_MOVE);
}
