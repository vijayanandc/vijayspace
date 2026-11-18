// Collapsible JSON section with a summary and a copy button. Large values are formatted only when opened and shown
// in part (with "Show all"), so a multi-megabyte payload does not lay out tens of thousands of lines.

import { Check, ChevronDown, ChevronRight, Copy } from 'lucide-react';
import { useMemo, useState } from 'react';

import type { JsonValue } from '../api/types';
import { COPY_FAILED, copyText } from '../lib/clipboard';
import { useToast } from './Toast';

function summary(value: JsonValue): string {
  if (Array.isArray(value)) return `${value.length} item${value.length === 1 ? '' : 's'}`;
  if (value && typeof value === 'object') {
    const n = Object.keys(value).length;
    return `${n} field${n === 1 ? '' : 's'}`;
  }
  return typeof value;
}

/** Characters shown before "Show all". */
export const JSON_PREVIEW_CHARS = 100_000;

const format = (value: JsonValue | undefined) => JSON.stringify(value, null, 2);

/** Collapsible JSON tree with copy. */
export function JsonView({ value, label, collapsed = false }: { value: JsonValue | undefined; label?: string; collapsed?: boolean }) {
  const [open, setOpen] = useState(!collapsed);
  const [all, setAll] = useState(false);
  const [copied, setCopied] = useState(false);
  const toast = useToast();
  const text = useMemo(() => (open && value !== undefined ? format(value) : ''), [open, value]);
  if (value === undefined) return null;
  const cut = !all && text.length > JSON_PREVIEW_CHARS;
  const copy = () => {
    copyText(text || format(value)).then(
      () => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      },
      () => toast.show('error', COPY_FAILED),
    );
  };
  return (
    <div className={`json-view${open ? ' json-open' : ''}`}>
      <div className="json-view-header">
        <button type="button" className="json-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          {open ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}
          <span className="json-label">{label ?? 'JSON'}</span>
          <span className="json-summary">{summary(value)}</span>
        </button>
        <button type="button" className="link-btn json-copy" onClick={copy} aria-label={`Copy ${label ?? 'JSON'}`}>
          {copied ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
      {open && <pre className="json-view-body">{cut ? `${text.slice(0, JSON_PREVIEW_CHARS)}\n…` : text}</pre>}
      {open && cut && (
        <button type="button" className="link-btn json-more" onClick={() => setAll(true)}>
          Show all ({Math.round(text.length / 1024).toLocaleString()} KB)
        </button>
      )}
    </div>
  );
}
