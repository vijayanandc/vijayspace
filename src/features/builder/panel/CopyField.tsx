// Read-only value with a copy button (webhook URL, event URL, secrets).

import { Check, Copy } from 'lucide-react';
import { useState } from 'react';

import { Button } from '../../../components/Button';
import { Field } from '../../../components/Field';

export function CopyField({ label, value, help }: { label: string; value: string; help?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard?.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };
  return (
    <Field label={label} help={help}>
      {(id) => (
        <div className="copy-row">
          <input id={id} className="input mono" readOnly value={value} onFocus={(e) => e.target.select()} />
          <Button icon={copied ? Check : Copy} onClick={copy}>
            {copied ? 'Copied' : 'Copy'}
          </Button>
        </div>
      )}
    </Field>
  );
}
