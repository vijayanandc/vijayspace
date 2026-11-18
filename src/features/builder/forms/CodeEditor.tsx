// Lightweight code editor: a real <textarea> with a line-number gutter that scrolls with it, Tab / Shift+Tab /
// Enter indentation, a resizable height, and validation issues highlighted on their lines.

import { CircleX } from 'lucide-react';
import { type KeyboardEvent, useLayoutEffect, useMemo, useRef } from 'react';

import type { Issue } from '../../../api/types';
import { type CodeEdit, applyEdit, codeProblems, formatLocation, indentEdit, lineCount, newlineEdit, normalizeNewlines, outdentEdit } from '../model/code';

interface Props {
  id: string;
  value: string;
  onChange: (v: string) => void;
  /** Validation issues of this field (messages like `line 2, column 9: …`). */
  issues?: Issue[];
  placeholder?: string;
  /** Initial height in lines (the editor can be resized). */
  rows?: number;
}

/** Line height and top padding in px; must match `.code-editor` in styles/code.css. */
const LINE_H = 18;
const PAD_Y = 8;

const plainKey = (e: KeyboardEvent) => !e.ctrlKey && !e.altKey && !e.metaKey;
/** Pressing a modifier alone (e.g. Shift for Shift+Tab) keeps "Escape, then Tab leaves" armed. */
const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta']);

/** Line numbers; re-rendered only when the line count or the lines with problems change, not on every keystroke. */
function Gutter({ count, errorKey }: { count: number; errorKey: string }) {
  const errors = useMemo(() => new Set(errorKey.split(',').filter(Boolean).map(Number)), [errorKey]);
  return useMemo(
    () => (
      <>
        {Array.from({ length: count }, (_, i) => (
          <div key={i} className={`code-ln${errors.has(i + 1) ? ' code-ln-error' : ''}`} data-line={i + 1}>
            {i + 1}
          </div>
        ))}
      </>
    ),
    [count, errors],
  );
}

export function CodeEditor({ id, value: raw, onChange, issues = [], placeholder, rows = 12 }: Props) {
  // Stored code may have CRLF line breaks; the textarea only ever reports LF, so offsets use the LF form.
  const value = useMemo(() => normalizeNewlines(raw), [raw]);
  const ref = useRef<HTMLTextAreaElement>(null);
  const gutter = useRef<HTMLDivElement>(null);
  const marks = useRef<HTMLDivElement>(null);
  const pendingSelection = useRef<[number, number] | null>(null);
  // Escape, then Tab, leaves the editor (keyboard users are never trapped).
  const tabLeaves = useRef(false);
  const { problems, general } = useMemo(() => codeProblems(issues, value), [issues, value]);
  const errorLines = [...new Set(problems.map((p) => p.line))].sort((a, b) => a - b);
  const count = lineCount(value);
  // The gutter is absolutely placed (the textarea alone sets the height), so its width is set here.
  const gutterWidth = Math.max(2, String(count).length) * 8 + 18;
  const messagesId = `${id}-problems`;
  const hintId = `${id}-hint`;
  const hasProblems = problems.length > 0 || general.length > 0;

  useLayoutEffect(() => {
    const selection = pendingSelection.current;
    if (selection && ref.current) {
      ref.current.setSelectionRange(selection[0], selection[1]);
      pendingSelection.current = null;
    }
  });

  const syncScroll = () => {
    const top = ref.current?.scrollTop ?? 0;
    for (const layer of [gutter.current, marks.current]) if (layer) layer.style.transform = `translateY(${-top}px)`;
  };

  const run = (edit: CodeEdit) => {
    const ta = ref.current;
    if (!ta) return;
    const next = applyEdit(value, edit);
    // Prefer the browser's own insertion, so the textarea's undo (Ctrl+Z) still covers the edit.
    ta.setSelectionRange(edit.start, edit.end);
    const native = typeof document.execCommand === 'function' && document.execCommand('insertText', false, edit.text);
    if (native && ta.value === next) {
      ta.setSelectionRange(edit.selectionStart, edit.selectionEnd);
      return;
    }
    pendingSelection.current = [edit.selectionStart, edit.selectionEnd];
    onChange(next);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    const { selectionStart: start, selectionEnd: end } = e.currentTarget;
    if (e.key === 'Escape') {
      tabLeaves.current = true;
      return;
    }
    if (MODIFIERS.has(e.key)) return;
    const leaving = tabLeaves.current;
    tabLeaves.current = false;
    if (e.key === 'Tab' && plainKey(e) && !leaving) {
      e.preventDefault();
      run(e.shiftKey ? outdentEdit(value, start, end) : indentEdit(value, start, end));
    } else if (e.key === 'Enter' && plainKey(e) && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      run(newlineEdit(value, start, end));
    }
  };

  return (
    <div className="code-field">
      <div className={`code-editor${hasProblems ? ' code-editor-invalid' : ''}`} style={{ paddingLeft: gutterWidth }}>
        <div className="code-gutter" aria-hidden="true" style={{ width: gutterWidth }}>
          <div ref={gutter} className="code-gutter-lines">
            <Gutter count={count} errorKey={errorLines.join(',')} />
          </div>
        </div>
        <div className="code-main">
          <div ref={marks} className="code-marks" aria-hidden="true">
            {errorLines.map((line) => (
              <div key={line} className="code-mark" style={{ top: PAD_Y + (line - 1) * LINE_H, height: LINE_H }} />
            ))}
          </div>
          <textarea
            ref={ref}
            id={id}
            className="code-input"
            value={value}
            placeholder={placeholder}
            rows={rows}
            wrap="off"
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
            autoCorrect="off"
            aria-invalid={hasProblems || undefined}
            aria-describedby={hasProblems ? `${messagesId} ${hintId}` : hintId}
            onChange={(e) => onChange(normalizeNewlines(e.target.value))}
            onKeyDown={onKeyDown}
            onScroll={syncScroll}
          />
        </div>
      </div>
      {hasProblems && (
        <ul id={messagesId} className="code-problems">
          {problems.map((p, i) => (
            <li key={`${p.line}:${i}`}>
              <CircleX size={14} aria-hidden="true" />
              <button type="button" className="code-problem-link" onClick={() => focusLine(ref.current, value, p.line, p.column)}>
                {formatLocation(p.line, p.column, p.message)}
              </button>
            </li>
          ))}
          {general.map((m) => (
            <li key={m}>
              <CircleX size={14} aria-hidden="true" />
              <span>{m}</span>
            </li>
          ))}
        </ul>
      )}
      <div id={hintId} className="code-hint">
        Tab indents · Shift+Tab outdents · Esc, then Tab or Shift+Tab, leaves the editor
      </div>
    </div>
  );
}

/** Puts the cursor on a 1-based line (and column) and scrolls it into view. */
function focusLine(ta: HTMLTextAreaElement | null, value: string, line: number, column = 1): void {
  if (!ta) return;
  const lines = value.split('\n');
  let offset = 0;
  for (let i = 0; i < line - 1 && i < lines.length; i += 1) offset += (lines[i]?.length ?? 0) + 1;
  const at = Math.min(offset + Math.max(0, column - 1), offset + (lines[line - 1]?.length ?? 0));
  ta.focus();
  ta.setSelectionRange(at, at);
  ta.scrollTop = Math.max(0, (line - 1) * LINE_H - ta.clientHeight / 2);
}
