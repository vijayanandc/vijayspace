// Collapsible JSON section with a summary and a copy button.

import { Check, ChevronDown, ChevronRight, Copy } from 'lucide-react';
import { useState } from 'react';

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

/** Collapsible JSON tree with copy. */
export function JsonView({ value, label, collapsed = false }: { value: JsonValue | undefined; label?: string; collapsed?: boolean }) {
  const [open, setOpen] = useState(!collapsed);
  const [copied, setCopied] = useState(false);
  const toast = useToast();
  if (value === undefined) return null;
  const text = JSON.stringify(value, null, 2);
  const copy = () => {
    copyText(text).then(
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
      {open && <pre className="json-view-body">{text}</pre>}
    </div>
  );
}
