import { describe, expect, it } from 'vitest';

import type { Issue } from '../../../api/types';
import {
  applyEdit,
  codeProblems,
  fieldIssues,
  formatLocation,
  indentEdit,
  lineCount,
  lineOf,
  newlineEdit,
  outdentEdit,
  parseLocated,
} from './code';

/** Applies an edit and marks the new selection with `|` (cursor) or `[` `]` (range). */
function show(value: string, edit: ReturnType<typeof indentEdit>): string {
  const next = applyEdit(value, edit);
  if (edit.selectionStart === edit.selectionEnd) return `${next.slice(0, edit.selectionStart)}|${next.slice(edit.selectionStart)}`;
  return `${next.slice(0, edit.selectionStart)}[${next.slice(edit.selectionStart, edit.selectionEnd)}]${next.slice(edit.selectionEnd)}`;
}

const issue = (path: string, message: string): Issue => ({ severity: 'error', path, code: 'field.invalid', message });

describe('code edits', () => {
  it('inserts two spaces at the cursor on Tab', () => {
    expect(show('let x = 1;', indentEdit('let x = 1;', 4, 4))).toBe('let   |x = 1;');
  });

  it('indents every line of a selection and keeps it selected', () => {
    const v = 'a\nb\nc';
    expect(show(v, indentEdit(v, 0, 3))).toBe('  [a\n  b]\nc');
    // A selection ending at a line start does not indent that line.
    expect(show(v, indentEdit(v, 0, 4))).toBe('  [a\n  b\n]c');
  });

  it('outdents the current line on Shift+Tab, moving the cursor with it', () => {
    expect(show('    x', outdentEdit('    x', 5, 5))).toBe('  x|');
    expect(show(' x', outdentEdit(' x', 2, 2))).toBe('x|');
    expect(show('\tx', outdentEdit('\tx', 1, 1))).toBe('|x');
    expect(show('x', outdentEdit('x', 1, 1))).toBe('x|');
  });

  it('outdents every selected line by at most one indent', () => {
    const v = '    a\n  b\nc';
    expect(show(v, outdentEdit(v, 4, v.length))).toBe('  [a\nb\nc]');
  });

  it('keeps the indentation of the current line on Enter', () => {
    const v = '  if x {\n    y';
    expect(show(v, newlineEdit(v, v.length, v.length))).toBe('  if x {\n    y\n    |');
    // Only the indentation before the cursor is repeated.
    expect(show('    z', newlineEdit('    z', 2, 2))).toBe('  \n  |  z');
    // A selection is replaced.
    expect(show('  ab', newlineEdit('  ab', 2, 4))).toBe('  \n  |');
  });

  it('counts lines and finds the line of an offset', () => {
    expect(lineCount('')).toBe(1);
    expect(lineCount('a\nb\n')).toBe(3);
    expect(lineOf('a\nb\nc', 0)).toBe(1);
    expect(lineOf('a\nb\nc', 2)).toBe(2);
    expect(lineOf('a\nb\nc', 5)).toBe(3);
  });
});

describe('issue mapping', () => {
  it('parses and formats the server’s "line N, column M:" messages', () => {
    expect(parseLocated("line 2, column 9: Unexpected ';'")).toEqual({ line: 2, column: 9, text: "Unexpected ';'" });
    expect(parseLocated('line 3: boom')).toEqual({ line: 3, column: undefined, text: 'boom' });
    expect(parseLocated('`code` is required')).toEqual({ text: '`code` is required' });
    expect(formatLocation(2, 9, "Unexpected ';'")).toBe("Line 2, column 9: Unexpected ';'");
    expect(formatLocation(undefined, undefined, 'boom')).toBe('boom');
  });

  it('picks the issues of one input field', () => {
    const issues = [issue('/steps/0/input/code', 'a'), issue('/steps/0/input/input', 'b'), issue('/steps/0/connection_id', 'c')];
    expect(fieldIssues(issues, 'code').map((i) => i.message)).toEqual(['a']);
    expect(fieldIssues(undefined, 'code')).toEqual([]);
  });

  it('places located issues on their line and clamps lines past the end', () => {
    const code = 'let x = 1;\nlet y = ;';
    expect(codeProblems([issue('/steps/0/input/code', "line 2, column 9: Unexpected ';'")], code)).toEqual({
      problems: [{ line: 2, column: 9, message: "Unexpected ';'" }],
      general: [],
    });
    expect(codeProblems([issue('/x', 'line 9: end of input')], code).problems[0]?.line).toBe(2);
  });

  it('places the template-marker issue on the lines holding markers', () => {
    const code = 'let a = 1;\nlet b = "{{ x }}";\nlet c = "{% y %}";';
    const { problems, general } = codeProblems([issue('/steps/0/input/code', 'The code contains `{{` or `{%`')], code);
    expect(problems.map((p) => p.line)).toEqual([2, 3]);
    expect(general).toEqual([]);
    expect(codeProblems([issue('/steps/0/input/code', '`code` is required')], '').general).toEqual(['`code` is required']);
  });
});
