import { describe, expect, it } from 'vitest';

import type { Step } from '../../../api/types';
import { createStep } from './factories';
import { SCRIPT_EXAMPLES, STARTER_SCRIPT, hasOwnCode, isScriptStep, scriptError, splitLogs, withExample } from './script';

describe('custom function steps', () => {
  it('creates an action step of core.script with the starter script', () => {
    const def = { schema_version: 1, trigger: { type: 'manual' as const }, steps: [] };
    const step = createStep(def, { kind: 'logic', type: 'custom_function' });
    expect(step).toMatchObject({ type: 'action', connector: 'core.script', action: 'run', name: 'Custom function', input: { code: STARTER_SCRIPT, input: {} } });
    expect(isScriptStep(step)).toBe(true);
    const http: Step = { key: 'h', type: 'action', connector: 'core.http', action: 'request', input: {} };
    expect(isScriptStep(http)).toBe(false);
  });

  it('never ships template markers in the starter or examples (the server rejects them)', () => {
    for (const code of [STARTER_SCRIPT, ...SCRIPT_EXAMPLES.map((e) => e.code)]) {
      expect(code).not.toMatch(/\{\{|\{%/);
    }
  });

  it('inserts an example and adds the input values it reads', () => {
    const sum = SCRIPT_EXAMPLES.find((e) => e.key === 'sum');
    if (!sum) throw new Error('missing example');
    const next = withExample({ code: 'x', input: { items: '{{ trigger.body.items }}', other: 1 }, language: 'rhai' }, sum);
    expect(next).toEqual({ code: sum.code, input: { items: '{{ trigger.body.items }}', other: 1 }, language: 'rhai' });
    expect(withExample({}, sum).input).toEqual({ items: '' });
  });

  it('knows when replacing the code would lose the user’s work', () => {
    expect(hasOwnCode(undefined)).toBe(false);
    expect(hasOwnCode('  ')).toBe(false);
    expect(hasOwnCode(STARTER_SCRIPT)).toBe(false);
    expect(hasOwnCode(SCRIPT_EXAMPLES[0]?.code)).toBe(false);
    expect(hasOwnCode('#{ a: 1 }')).toBe(true);
  });
});

describe('script results', () => {
  it('splits print lines from the output', () => {
    expect(splitLogs({ total: 3, _logs: ['a', 'b'] })).toEqual({ logs: ['a', 'b'], rest: { total: 3 } });
    expect(splitLogs({ total: 3 })).toEqual({ logs: [], rest: { total: 3 } });
    expect(splitLogs(undefined)).toEqual({ logs: [], rest: undefined });
  });

  it('reads line and column from details, or else from the message', () => {
    expect(scriptError({ code: 'script.error', message: 'line 2, column 1: boom', details: { line: 2, column: 1 } })).toEqual({ code: 'script.error', line: 2, column: 1, text: 'boom' });
    expect(scriptError({ code: 'script.error', message: 'line 4, column 7: no items', retryable: false })).toEqual({ code: 'script.error', line: 4, column: 7, text: 'no items' });
    expect(scriptError({ code: 'script.limit_exceeded', message: 'the script ran for too long', details: { line: null, column: null } })).toEqual({
      code: 'script.limit_exceeded', line: undefined, column: undefined, text: 'the script ran for too long',
    });
    expect(scriptError({ code: 'http.status', message: 'line 1: x' })).toBeNull();
    expect(scriptError(null)).toBeNull();
  });
});
