// Right panel for the selected node: header, then Configure | Settings | Test tabs.

import { type KeyboardEvent, useState } from 'react';

import { type Tab, Tabs } from '../../../components/Tabs';
import { useBuilder } from '../BuilderContext';
import { stepLabel, triggerLabel } from '../canvas/labels';
import { stepNodeId } from '../model/layout';
import { TRIGGER_ID } from '../model/paths';
import { isScriptStep } from '../../../domain/script';
import { findStep } from '../../../domain/tree';
import { ActionPanel } from './ActionPanel';
import { DecisionPanel } from './DecisionPanel';
import { DelayPanel, SetVariablePanel, StopPanel } from './LogicPanels';
import { NodeIssues, PanelHeader } from './PanelHeader';
import { ScriptPanel } from './ScriptPanel';
import { StepSettings } from './StepSettings';
import { TestStepBox } from './TestStepBox';
import { TriggerSettings, TriggerTest } from './TriggerExtras';
import { TriggerPanel } from './TriggerPanel';
import { can, useRole } from '../../auth';

type PanelTab = 'configure' | 'settings' | 'test';

const ALL_TABS: Tab<PanelTab>[] = [
  { key: 'configure', label: 'Configure' },
  { key: 'settings', label: 'Settings' },
  { key: 'test', label: 'Test' },
];

export function SidePanel() {
  const role = useRole();
  const { selected, def, manifests, select, readOnly } = useBuilder();
  // The tab belongs to the node it was chosen on: selecting another node (or none) starts again on Configure.
  // A key rename keeps the tab because StepSettings reports the new key before selecting it.
  const [state, setState] = useState<{ id: string | null; tab: PanelTab }>({ id: selected, tab: 'configure' });
  if (state.id !== selected) setState({ id: selected, tab: 'configure' });
  if (!selected) return null;
  // Testing runs things: not for viewers.
  const canTest = can(role, 'flow.edit');
  const visible = canTest ? ALL_TABS : ALL_TABS.filter((t) => t.key !== 'test');
  const tab = state.id === selected ? state.tab : 'configure';
  const setTab = (t: PanelTab) => setState({ id: selected, tab: t });
  const close = () => select(null);
  // Escape anywhere in the panel closes it (as on the canvas) and puts focus back on its card. Inner popups that
  // handle Escape themselves (the variable picker, menus) mark it handled first.
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    e.preventDefault();
    const card = selected === TRIGGER_ID ? TRIGGER_ID : stepNodeId(selected);
    close();
    window.requestAnimationFrame(() => document.querySelector<HTMLElement>(`.react-flow__node[data-id="${CSS.escape(card)}"]`)?.focus());
  };

  if (selected === TRIGGER_ID) {
    return (
      <aside className="side-panel" aria-label="Trigger settings" onKeyDown={onKeyDown}>
        <PanelHeader label={triggerLabel(def.trigger, manifests)} onClose={close} />
        <Tabs label="Trigger panel" tabs={visible} value={tab} onChange={setTab} className="panel-tabs" />
        <fieldset className="panel-scroll panel-fieldset" disabled={readOnly}>
          {tab === 'configure' && (
            <>
              <NodeIssues id={TRIGGER_ID} />
              <TriggerPanel />
            </>
          )}
          {tab === 'settings' && <TriggerSettings />}
          {tab === 'test' && canTest && <TriggerTest />}
        </fieldset>
      </aside>
    );
  }

  const step = findStep(def, selected);
  if (!step) return null;
  const tabs = step.type === 'action' ? visible : visible.filter((t) => t.key !== 'test');
  const shown = tabs.some((t) => t.key === tab) ? tab : 'configure';
  // Keyed by step: nothing typed or run in one step's panel (test results, JSON drafts, rows) carries into another.
  return (
    <aside key={step.key} className="side-panel" aria-label="Step settings" onKeyDown={onKeyDown}>
      <PanelHeader label={stepLabel(step, manifests)} step={step} onClose={close} />
      <Tabs label="Step panel" tabs={tabs} value={shown} onChange={setTab} className="panel-tabs" />
      {/* Viewers read the settings; a disabled fieldset disables every control inside it. */}
      <fieldset className="panel-scroll panel-fieldset" disabled={readOnly}>
        {shown === 'configure' && (
          <>
            <NodeIssues id={step.key} />
            {step.type === 'action' && (isScriptStep(step) ? <ScriptPanel step={step} /> : <ActionPanel step={step} />)}
            {step.type === 'decision' && <DecisionPanel step={step} />}
            {step.type === 'delay' && <DelayPanel step={step} />}
            {step.type === 'set_variable' && <SetVariablePanel step={step} />}
            {step.type === 'stop' && <StopPanel step={step} />}
          </>
        )}
        {shown === 'settings' && <StepSettings key={step.key} step={step} onRenamed={(key) => setState({ id: key, tab: 'settings' })} />}
        {shown === 'test' && step.type === 'action' && <TestStepBox stepKey={step.key} />}
      </fieldset>
    </aside>
  );
}
