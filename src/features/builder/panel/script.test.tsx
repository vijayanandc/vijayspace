import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ConnectorManifest, Flow, FlowDefinition, Issue, Step } from '../../../api/types';
import { renderWithProviders } from '../../../test/render';
import { type BuilderCtx, TestBuilderProvider } from '../BuilderContext';
import { CodeEditor } from '../../../forms/CodeEditor';
import { SCRIPT_EXAMPLES } from '../../../domain/script';
import { findStep } from '../../../domain/tree';
import { SidePanel } from './SidePanel';

const script: ConnectorManifest = {
  key: 'core.script', name: 'Custom function', description: '', icon: 'ƒ', category: 'Logic', auth: { kind: 'none', fields: [] }, triggers: [],
  actions: [{
    key: 'run', name: 'Custom function', description: 'Run a Rhai script on the step’s input.',
    fields: [
      { key: 'code', label: 'Code', type: 'code', required: true, mappable: false },
      { key: 'input', label: 'Input values', type: 'key_value' },
      { key: 'language', label: 'Language', type: 'select', options: [{ value: 'rhai', label: 'Rhai' }], advanced: true, mappable: false },
    ],
  }],
};

const fnStep = (code: string): Step => ({ key: 'fn', name: 'Total', type: 'action', connector: 'core.script', action: 'run', input: { code, input: {} } });

function renderScript(code: string, issues: Issue[] = []) {
  let latest: FlowDefinition = { schema_version: 1, trigger: { type: 'webhook' }, steps: [fnStep(code)] };
  function Harness() {
    const [def, setDef] = useState(latest);
    latest = def;
    const ctx = {
      flowId: 'flw_1', flow: { trigger: { kind: 'webhook', token: 't', is_active: false } } as Flow, def, manifests: [script], selected: 'fn', select: vi.fn(),
      apply: (d: FlowDefinition) => setDef(d), issues: new Map([['fn', issues]]), flush: async () => true, readOnly: false, moving: null, startMove: vi.fn(), deleteStep: vi.fn(),
    } as unknown as BuilderCtx;
    return (
      <TestBuilderProvider value={ctx}>
        <SidePanel />
      </TestBuilderProvider>
    );
  }
  renderWithProviders(<Harness />);
  return {
    input: () => {
      const step = findStep(latest, 'fn');
      return step?.type === 'action' ? step.input : undefined;
    },
  };
}

describe('custom function panel', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('highlights the line named by an issue on /steps/0/input/code and shows the message under the editor', () => {
    renderScript('let x = 1;\nlet y = ;\n#{ x: x }', [
      { severity: 'error', path: '/steps/0/input/code', code: 'field.invalid', message: "line 2, column 9: Unexpected ';'" },
    ]);
    const editor = screen.getByLabelText(/^Code/);
    expect(editor.tagName).toBe('TEXTAREA');
    expect(editor).toHaveAttribute('aria-invalid', 'true');
    const gutter = document.querySelectorAll('.code-ln');
    expect(gutter).toHaveLength(3);
    expect(document.querySelector('[data-line="2"]')).toHaveClass('code-ln-error');
    expect(document.querySelector('[data-line="1"]')).not.toHaveClass('code-ln-error');
    expect(document.querySelectorAll('.code-mark')).toHaveLength(1);
    const [problemsId, hintId] = (editor.getAttribute('aria-describedby') ?? '').split(' ');
    expect(document.getElementById(problemsId ?? '')).toHaveTextContent("Line 2, column 9: Unexpected ';'");
    expect(document.getElementById(hintId ?? '')).toHaveTextContent(/Esc, then Tab or Shift\+Tab, leaves the editor/);
  });

  it('does not put an issue of an input value named “code” on the editor', () => {
    renderScript('#{ x: 1 }', [{ severity: 'error', path: '/steps/0/input/input/code', code: 'field.invalid', message: 'line 1: not the editor' }]);
    expect(screen.getByLabelText(/^Code/)).not.toHaveAttribute('aria-invalid');
    expect(document.querySelectorAll('.code-ln-error')).toHaveLength(0);
  });

  it('inserts an example and the input values it reads', () => {
    const { input } = renderScript('');
    fireEvent.click(screen.getByRole('button', { name: /Insert example/ }));
    fireEvent.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: /Sum line items/ }));
    expect(input()).toMatchObject({ code: SCRIPT_EXAMPLES[0]?.code, input: { items: '' } });
    expect(screen.getByLabelText('Input values key 1')).toHaveValue('items');
  });

  it('continues in the editor, at the top, after inserting an example', async () => {
    renderScript('');
    fireEvent.click(screen.getByRole('button', { name: /Insert example/ }));
    fireEvent.click(within(screen.getByRole('menu')).getByRole('menuitem', { name: /Format a date/ }));
    const editor = screen.getByLabelText(/^Code/) as HTMLTextAreaElement;
    await waitFor(() => expect(document.activeElement).toBe(editor));
    expect(editor.selectionStart).toBe(0);
  });

  it('explains input, context, output, throw and print, and lists the helpers', () => {
    renderScript('');
    fireEvent.click(screen.getByRole('button', { name: /How custom functions work/ }));
    for (const text of ['context.now', 'throw "message"', '_logs', 'hmac_sha256(text, key)', '{{ steps.fn.total }}', 'total(input.items)']) {
      expect(screen.getAllByText((_, el) => el?.tagName === 'CODE' && el.textContent === text).length).toBeGreaterThan(0);
    }
  });

  it('shows print lines as a log and script errors with line and column in the Test tab', async () => {
    const results = [
      { input: { code: 'x' }, output: { total: 105.5, _logs: ['2 item(s)'] } },
      { input: { code: 'x' }, error: { code: 'script.error', message: 'line 4, column 7: no items', retryable: false } },
    ];
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify(results.shift()))));
    renderScript('#{ total: 1 }');
    fireEvent.click(screen.getByRole('tab', { name: 'Test' }));
    fireEvent.click(screen.getByRole('button', { name: 'Test step' }));
    const log = await screen.findByRole('region', { name: 'Log' });
    expect(within(log).getByText('2 item(s)')).toBeInTheDocument();
    expect(screen.getByText(/"total": 105.5/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Test step' }));
    expect(await screen.findByText(/Line 4, column 7: no items/)).toBeInTheDocument();
    expect(screen.getByText('Script error')).toBeInTheDocument();
  });
});

describe('CodeEditor keys', () => {
  function Editor({ initial }: { initial: string }) {
    const [value, setValue] = useState(initial);
    return <CodeEditor id="code" value={value} onChange={setValue} />;
  }

  it('indents with Tab, outdents with Shift+Tab, keeps indentation on Enter, and lets Esc then Tab leave', () => {
    renderWithProviders(<Editor initial={'if x {\n  y'} />);
    const ta = document.getElementById('code') as HTMLTextAreaElement;
    ta.setSelectionRange(ta.value.length, ta.value.length);
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(ta.value).toBe('if x {\n  y\n  ');
    fireEvent.keyDown(ta, { key: 'Tab' });
    expect(ta.value).toBe('if x {\n  y\n    ');
    fireEvent.keyDown(ta, { key: 'Tab', shiftKey: true });
    expect(ta.value).toBe('if x {\n  y\n  ');
    expect(ta.selectionStart).toBe(ta.value.length);
    expect(document.querySelectorAll('.code-ln')).toHaveLength(3);
    fireEvent.keyDown(ta, { key: 'Escape' });
    const leave = fireEvent.keyDown(ta, { key: 'Tab' });
    expect(leave).toBe(true); // not prevented: focus may move on
    expect(ta.value).toBe('if x {\n  y\n  ');
  });

  it('lets Esc then Shift+Tab leave too (pressing Shift alone keeps the way out open)', () => {
    renderWithProviders(<Editor initial={'a'} />);
    const ta = document.getElementById('code') as HTMLTextAreaElement;
    fireEvent.keyDown(ta, { key: 'Escape' });
    fireEvent.keyDown(ta, { key: 'Shift', shiftKey: true });
    expect(fireEvent.keyDown(ta, { key: 'Tab', shiftKey: true })).toBe(true);
    expect(fireEvent.keyDown(ta, { key: 'Tab' })).toBe(false); // armed once: Tab indents again
    fireEvent.keyDown(ta, { key: 'Escape' });
    fireEvent.blur(ta);
    expect(fireEvent.keyDown(ta, { key: 'Tab' })).toBe(false); // leaving the editor disarms it
  });

  it('shows CRLF code with LF line breaks, so lines and offsets line up', () => {
    const changes: string[] = [];
    renderWithProviders(<CodeEditor id="code" value={'let a = 1;\r\nlet b = 2;\r\n'} onChange={(v) => changes.push(v)} />);
    const ta = document.getElementById('code') as HTMLTextAreaElement;
    expect(ta.value).toBe('let a = 1;\nlet b = 2;\n');
    expect(document.querySelectorAll('.code-ln')).toHaveLength(3);
    ta.setSelectionRange(10, 10);
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(changes.at(-1)).toBe('let a = 1;\n\nlet b = 2;\n');
  });
});
