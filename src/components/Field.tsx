// Form field wrapper: label, required mark, help text with inline code, error text and header actions. The control
// (the element given the field's id) is linked to the help and error text and marked required or invalid, so screen
// readers announce them with it.

import { type ReactNode, useId, useLayoutEffect, useRef } from 'react';

import { inlineCodeParts } from '../lib/format';

interface Props {
  label: string;
  required?: boolean;
  help?: string;
  error?: string;
  /** Render prop receives the id to put on the control. */
  children: (id: string) => ReactNode;
  actions?: ReactNode;
}

/**
 * Adds the field's own description ids and states to `control`, keeping what the control sets itself (a code editor
 * describes its own problems): only what this field added before is replaced or removed.
 */
function link(control: HTMLElement, describedBy: string[], invalid: boolean, required: boolean): void {
  const before = (control.dataset.fieldDescribedby ?? '').split(' ').filter(Boolean);
  const own = (control.getAttribute('aria-describedby') ?? '').split(' ').filter((t) => t && !before.includes(t));
  const all = [...own, ...describedBy];
  if (all.length) control.setAttribute('aria-describedby', all.join(' '));
  else control.removeAttribute('aria-describedby');
  control.dataset.fieldDescribedby = describedBy.join(' ');
  for (const [attr, key, on] of [['aria-invalid', 'fieldInvalid', invalid], ['aria-required', 'fieldRequired', required]] as const) {
    if (on) {
      control.setAttribute(attr, 'true');
      control.dataset[key] = '1';
    } else if (control.dataset[key]) {
      control.removeAttribute(attr);
      delete control.dataset[key];
    }
  }
}

export function Field({ label, required, help, error, children, actions }: Props) {
  const id = useId();
  const ref = useRef<HTMLDivElement>(null);
  const helpId = `${id}-help`;
  const errorId = `${id}-error`;
  // Every render: the control can be any element a call site renders with the id (input, select, custom inputs).
  useLayoutEffect(() => {
    const control = ref.current?.querySelector<HTMLElement>(`[id="${id}"]`);
    if (!control) return;
    const describedBy = [help ? helpId : null, error ? errorId : null].filter((x): x is string => x !== null);
    link(control, describedBy, Boolean(error), Boolean(required));
  });
  return (
    <div ref={ref} className={`field${error ? ' field-error' : ''}`}>
      <div className="field-label-row">
        <label htmlFor={id} className="field-label">
          {label}
          {required && <span className="required" aria-hidden="true"> *</span>}
        </label>
        {actions}
      </div>
      {children(id)}
      {help && (
        <div id={helpId} className="field-help">
          {inlineCodeParts(help).map((p, i) => (p.code ? <code key={i}>{p.text}</code> : <span key={i}>{p.text}</span>))}
        </div>
      )}
      {error && (
        <div id={errorId} className="field-error-text" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
