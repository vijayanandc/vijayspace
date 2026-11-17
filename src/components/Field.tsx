import { type ReactNode, useId } from 'react';

interface Props {
  label: string;
  required?: boolean;
  help?: string;
  error?: string;
  /** Render prop receives the id to put on the control. */
  children: (id: string) => ReactNode;
  actions?: ReactNode;
}

export function Field({ label, required, help, error, children, actions }: Props) {
  const id = useId();
  return (
    <div className={`field${error ? ' field-error' : ''}`}>
      <div className="field-label-row">
        <label htmlFor={id} className="field-label">
          {label}
          {required && <span className="required" aria-hidden="true"> *</span>}
        </label>
        {actions}
      </div>
      {children(id)}
      {help && <div className="field-help">{help}</div>}
      {error && <div className="field-error-text" role="alert">{error}</div>}
    </div>
  );
}
