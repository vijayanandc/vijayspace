// Code editor helpers (pure): indent / outdent / newline edits, line numbers and validation issue mapping.

import type { Issue } from '../api/types';

export const INDENT = '  ';

/** Replace `[start, end)` with `text`, then select `[selectionStart, selectionEnd]` (offsets in the new value). */
export interface CodeEdit {
  start: number;
  end: number;
  text: string;
  selectionStart: number;
  selectionEnd: number;
}

/** Applies an edit to a string. */
export function applyEdit(value: string, edit: CodeEdit): string {
  return value.slice(0, edit.start) + edit.text + value.slice(edit.end);
}

/** Offset where the line containing `offset` starts. */
export function lineStart(value: string, offset: number): number {
  return value.lastIndexOf('\n', Math.max(0, offset - 1)) + 1;
}

/** Offset where the line containing `offset` ends (before its newline). */
export function lineEnd(value: string, offset: number): number {
  const i = value.indexOf('\n', offset);
  return i === -1 ? value.length : i;
}

/** 1-based line number of `offset`. */
export function lineOf(value: string, offset: number): number {
  let n = 1;
  for (let i = 0; i < Math.min(offset, value.length); i += 1) if (value[i] === '\n') n += 1;
  return n;
}

/** CRLF / CR → LF. A textarea reports LF line breaks, so offsets only match a normalised value. */
export function normalizeNewlines(value: string): string {
  return value.includes('\r') ? value.replace(/\r\n?/g, '\n') : value;
}

export function lineCount(value: string): number {
  return value.split('\n').length;
}

/** The lines touched by a selection (a selection ending at a line start does not touch that line). */
function touchedBlock(value: string, start: number, end: number): { from: number; to: number } {
  const last = end > start && value[end - 1] === '\n' ? end - 1 : end;
  return { from: lineStart(value, start), to: lineEnd(value, last) };
}

/** Tab: inserts an indent at the cursor, or indents every line of a selection. */
export function indentEdit(value: string, start: number, end: number, unit = INDENT): CodeEdit {
  if (start === end) return { start, end, text: unit, selectionStart: start + unit.length, selectionEnd: start + unit.length };
  const { from, to } = touchedBlock(value, start, end);
  const lines = value.slice(from, to).split('\n');
  const text = lines.map((l) => unit + l).join('\n');
  return { start: from, end: to, text, selectionStart: start + unit.length, selectionEnd: end + unit.length * lines.length };
}

/** Shift+Tab: removes up to one indent (spaces or a tab) from the start of every touched line. */
export function outdentEdit(value: string, start: number, end: number, unit = INDENT): CodeEdit {
  const { from, to } = touchedBlock(value, start, end);
  const lines = value.slice(from, to).split('\n');
  const removed = lines.map((l) => (l.startsWith('\t') ? 1 : Math.min(unit.length, /^ */.exec(l)?.[0].length ?? 0)));
  const text = lines.map((l, i) => l.slice(removed[i] ?? 0)).join('\n');
  // Moves an old offset left by the indentation removed before it.
  const map = (offset: number): number => {
    let shift = 0;
    let at = from;
    lines.forEach((l, i) => {
      if (at <= offset) shift += Math.min(removed[i] ?? 0, offset - at);
      at += l.length + 1;
    });
    return offset - shift;
  };
  return { start: from, end: to, text, selectionStart: map(start), selectionEnd: map(end) };
}

/** Enter: a newline that keeps the current line's indentation (up to the cursor). */
export function newlineEdit(value: string, start: number, end: number): CodeEdit {
  const from = lineStart(value, start);
  const indent = /^[ \t]*/.exec(value.slice(from, start))?.[0] ?? '';
  const text = `\n${indent}`;
  return { start, end, text, selectionStart: start + text.length, selectionEnd: start + text.length };
}

/** A validation problem placed on a line (and column, when known). */
export interface CodeProblem {
  line: number;
  column?: number;
  message: string;
}

const LOCATED = /^line (\d+)(?:, column (\d+))?: ([\s\S]*)$/i;

/** Splits `line N, column M: message` (the server's script error format) into its parts. */
export function parseLocated(message: string): { line?: number; column?: number; text: string } {
  const m = LOCATED.exec(message.trim());
  if (!m) return { text: message };
  return { line: Number(m[1]), column: m[2] ? Number(m[2]) : undefined, text: m[3] ?? '' };
}

/** "Line 2, column 9: Unexpected ';'". */
export function formatLocation(line: number | undefined, column: number | undefined, text: string): string {
  if (line === undefined) return text;
  return `Line ${line}${column !== undefined ? `, column ${column}` : ''}: ${text}`;
}

const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Issues on one top-level input field of a step: `/steps/N/input/<key>` (or `…/otherwise/N/input/<key>` inside a
 * decision). A nested value with the same name, such as `/steps/0/input/input/code`, is not this field.
 */
export function fieldIssues(issues: Issue[] | undefined, key: string): Issue[] {
  const own = new RegExp(`/(?:steps|otherwise)/\\d+/input/${escapeRegExp(key)}$`);
  return (issues ?? []).filter((i) => own.test(i.path));
}

/** Lines holding template markers (`{{`, `{%`), which code may not contain. */
export function markerLines(code: string): number[] {
  return code.split('\n').flatMap((l, i) => (l.includes('{{') || l.includes('{%') ? [i + 1] : []));
}

/**
 * Places a field's issues on lines: `line N…` messages on line N; a message without a line
 * (the template-marker check) on each line with a marker. Lines past the end are clamped.
 */
export function codeProblems(issues: Issue[], code: string): { problems: CodeProblem[]; general: string[] } {
  const last = lineCount(code);
  const problems: CodeProblem[] = [];
  const general: string[] = [];
  for (const issue of issues) {
    const loc = parseLocated(issue.message);
    if (loc.line !== undefined) {
      problems.push({ line: Math.min(Math.max(1, loc.line), last), column: loc.column, message: loc.text });
      continue;
    }
    const lines = markerLines(code);
    if (lines.length === 0) general.push(issue.message);
    else lines.forEach((line, i) => problems.push({ line, message: i === 0 ? issue.message : 'Template marker' }));
  }
  return { problems, general };
}
