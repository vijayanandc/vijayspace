// "New flow" dialog: a name and the trigger, chosen from four tiles.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { CircleCheck } from 'lucide-react';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { TriggerType } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox } from '../../components/Empty';
import { Field } from '../../components/Field';
import { starterDefinition } from '../builder/model/factories';

export const TRIGGER_CHOICES: { type: TriggerType; label: string; description: string }[] = [
  { type: 'webhook', label: 'Webhook', description: 'An HTTP request to a unique URL starts the flow' },
  { type: 'app', label: 'App event', description: 'A record changes in Zoho CRM, Appblocks, Forma…' },
  { type: 'schedule', label: 'Schedule', description: 'Run once, every N minutes, or on a cron schedule' },
  { type: 'manual', label: 'Manual', description: 'Run on demand with “Run now” or the API' },
];

export function NewFlowDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [name, setName] = useState('');
  const [trigger, setTrigger] = useState<TriggerType>('webhook');
  const navigate = useNavigate();
  const qc = useQueryClient();
  const create = useMutation({
    mutationFn: () => endpoints.createFlow({ name: name.trim(), definition: starterDefinition(trigger) }),
    onSuccess: (flow) => {
      void qc.invalidateQueries({ queryKey: qk.flows });
      onClose();
      navigate(`/flows/${flow.id}`);
    },
  });
  return (
    <Dialog
      title="New flow"
      subtitle="Name your flow and choose what starts it. You can change the trigger later."
      open={open}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!name.trim() || create.isPending} onClick={() => create.mutate()}>
            Create
          </Button>
        </>
      }
    >
      <Field label="Name" required>
        {(id) => (
          <input
            id={id}
            className="input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && name.trim() && !create.isPending && create.mutate()}
            placeholder="e.g. Deal won → FSM request"
          />
        )}
      </Field>
      <fieldset className="choice-grid">
        <legend className="field-label">
          Trigger<span className="required" aria-hidden="true"> *</span>
        </legend>
        {TRIGGER_CHOICES.map((c) => (
          <label key={c.type} className={`choice${trigger === c.type ? ' choice-active' : ''}`}>
            <input type="radio" name="trigger" value={c.type} checked={trigger === c.type} onChange={() => setTrigger(c.type)} />
            <AppIcon app={`trigger:${c.type}`} size={32} decorative />
            <span className="choice-text">
              <strong>{c.label}</strong>
              <span className="choice-desc">{c.description}</span>
            </span>
            {trigger === c.type && <CircleCheck size={18} className="choice-check" aria-hidden="true" />}
          </label>
        ))}
      </fieldset>
      {create.error && <ErrorBox message={errorMessage(create.error)} />}
    </Dialog>
  );
}
