// "Configure" tab of a custom function (`core.script`): help and examples, then the code editor and input values.

import { useRef } from 'react';

import type { ActionStep, JsonObject } from '../../../api/types';
import { MessageBar } from '../../../components/MessageBar';
import { useBuilder } from '../BuilderContext';
import { DynamicForm } from '../../../forms/DynamicForm';
import { SCRIPT_CONNECTOR, type ScriptExample, hasOwnCode, withExample } from '../../../domain/script';
import { updateStep } from '../../../domain/tree';
import { useVariables } from './useVariables';
import { ScriptHelp } from './ScriptHelp';

export function ScriptPanel({ step }: { step: ActionStep }) {
  const { def, apply, manifests, issues } = useBuilder();
  const spec = manifests.find((m) => m.key === SCRIPT_CONNECTOR)?.actions.find((a) => a.key === step.action);
  const variables = useVariables(step.key);
  const input: JsonObject = step.input ?? {};
  const setInput = (next: JsonObject, tag?: string) => apply(updateStep(def, step.key, (s) => (s.type === 'action' ? { ...s, input: next } : s)), tag);
  const form = useRef<HTMLElement>(null);
  const insert = (example: ScriptExample) => {
    if (hasOwnCode(input.code) && !window.confirm(`Replace your code with the “${example.label}” example? You can undo this.`)) return;
    setInput(withExample(input, example));
    // The menu item that had focus is gone: continue in the editor, at the top of the new code.
    requestAnimationFrame(() => {
      const editor = form.current?.querySelector<HTMLTextAreaElement>('textarea.code-input');
      if (!editor) return;
      editor.focus();
      editor.setSelectionRange(0, 0);
      editor.scrollTop = 0;
    });
  };

  if (!spec) {
    return (
      <div className="panel-body">
        <MessageBar intent="warning">This server does not offer custom functions (the core.script app is missing).</MessageBar>
      </div>
    );
  }
  return (
    <div className="panel-body">
      <section className="panel-section">
        <h3 className="section-title">Custom function</h3>
        <p className="field-help script-intro">{spec.description}</p>
        <ScriptHelp stepKey={step.key} onInsert={insert} />
      </section>
      <section className="panel-section" ref={form}>
        <DynamicForm
          fields={spec.fields}
          values={input}
          onChange={(next: JsonObject) => setInput(next, `input:${step.key}`)}
          env={{ connector: step.connector, connectionId: null, requiresConnection: false, variables, issues: issues.get(step.key) }}
        />
      </section>
    </div>
  );
}
