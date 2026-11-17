// Fluent-style On/Off switch. A real checkbox inside a <label>, so clicking the label toggles it.

interface Props {
  checked: boolean;
  onChange: () => void;
  /** Accessible name of the checkbox. */
  label: string;
  disabled?: boolean;
  title?: string;
  /** Show the "On" / "Off" text next to the track. */
  showText?: boolean;
}

export function Switch({ checked, onChange, label, disabled = false, title, showText = true }: Props) {
  return (
    <label className={`switch${disabled ? ' switch-disabled' : ''}`} title={title}>
      <input type="checkbox" checked={checked} disabled={disabled} onChange={onChange} aria-label={label} />
      <span className="switch-track" aria-hidden="true" />
      {showText && <span className="switch-label">{checked ? 'On' : 'Off'}</span>}
    </label>
  );
}
