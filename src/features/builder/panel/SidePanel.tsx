import { useBuilder } from '../BuilderContext';
import { stepLabel } from '../canvas/nodes';
import { TRIGGER_ID } from '../model/paths';
import { findStep } from '../model/tree';
import { ActionPanel } from './ActionPanel';
import { DecisionPanel } from './DecisionPanel';
import { DelayPanel, SetVariablePanel, StopPanel } from './LogicPanels';
import { StepHeader } from './StepHeader';
import { TriggerPanel } from './TriggerPanel';

export function SidePanel() {
  const { selected, def, manifests, select } = useBuilder();
  if (!selected) {
    return (
      <aside className="side-panel side-panel-empty" aria-label="Step settings">
        <p className="muted">Select the trigger or a step to configure it.</p>
        <p className="muted small">Drag steps from the palette onto a + on the canvas. Drag a step onto another + to move it.</p>
      </aside>
    );
  }
  if (selected === TRIGGER_ID) {
    return (
      <aside className="side-panel" aria-label="Trigger settings">
        <div className="side-panel-title">
          <h2>Trigger</h2>
          <button type="button" className="icon-btn" aria-label="Close panel" onClick={() => select(null)}>
            ×
          </button>
        </div>
        <TriggerPanel />
      </aside>
    );
  }
  const step = findStep(def, selected);
  if (!step) return null;
  return (
    <aside className="side-panel" aria-label="Step settings">
      <div className="side-panel-title">
        <h2>{stepLabel(step, manifests).title}</h2>
        <button type="button" className="icon-btn" aria-label="Close panel" onClick={() => select(null)}>
          ×
        </button>
      </div>
      <StepHeader key={step.key} step={step} />
      {step.type === 'action' && <ActionPanel step={step} />}
      {step.type === 'decision' && <DecisionPanel step={step} />}
      {step.type === 'delay' && <DelayPanel step={step} />}
      {step.type === 'set_variable' && <SetVariablePanel step={step} />}
      {step.type === 'stop' && <StopPanel step={step} />}
    </aside>
  );
}
