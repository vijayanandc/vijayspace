// Install wizard: pick (or create) one connection per recipe slot, then install the recipe's flows as drafts.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { CircleCheck, Workflow } from 'lucide-react';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Recipe } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox } from '../../components/Empty';
import { MessageBar } from '../../components/MessageBar';
import { ConnectionPicker } from '../connections';
import { RecipeApps } from './RecipeApps';

interface Props {
  recipe: Recipe;
  name: (key: string) => string;
  onClose: () => void;
}

export function InstallWizard({ recipe, name, onClose }: Props) {
  const [chosen, setChosen] = useState<Record<string, string>>({});
  const navigate = useNavigate();
  const qc = useQueryClient();
  const done = recipe.slots.filter((s) => Boolean(chosen[s.slot])).length;
  const complete = done === recipe.slots.length;
  const install = useMutation({
    mutationFn: () => endpoints.installRecipe(recipe.key, chosen),
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: qk.flows });
      onClose();
      const first = res.flow_ids[0];
      navigate(res.flow_ids.length === 1 && first ? `/flows/${first}` : '/flows');
    },
  });
  return (
    <Dialog
      title={`Install: ${recipe.name}`}
      open
      onClose={onClose}
      wide
      footer={
        <>
          <span className="dialog-footer-note">
            {done} of {recipe.slots.length} connection{recipe.slots.length === 1 ? '' : 's'} chosen
          </span>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!complete || install.isPending} onClick={() => install.mutate()}>
            Install {recipe.flows.length} flow{recipe.flows.length === 1 ? '' : 's'}
          </Button>
        </>
      }
    >
      <div className="wizard-summary">
        <RecipeApps apps={recipe.apps} name={name} size={40} />
        <p>{recipe.description}</p>
      </div>
      <h3 className="wizard-step">
        <span className="wizard-num">1</span> Connect your apps
      </h3>
      <ol className="slot-list">
        {recipe.slots.map((slot) => (
          <li key={slot.slot} className={`slot${chosen[slot.slot] ? ' slot-done' : ''}`}>
            <div className="slot-head">
              <AppIcon app={slot.connector} name={name(slot.connector)} size={28} decorative />
              <strong>{slot.label}</strong>
              <span className="cell-muted">{name(slot.connector)}</span>
              {chosen[slot.slot] && <CircleCheck size={16} className="slot-check" aria-hidden="true" />}
            </div>
            <ConnectionPicker connector={slot.connector} value={chosen[slot.slot] ?? null} onChange={(id) => setChosen((prev) => ({ ...prev, [slot.slot]: id ?? '' }))} />
          </li>
        ))}
      </ol>
      <h3 className="wizard-step">
        <span className="wizard-num">2</span> Review the flows
      </h3>
      <ul className="wizard-flows">
        {recipe.flows.map((f) => (
          <li key={f.name}>
            <Workflow size={16} aria-hidden="true" />
            <span>
              <strong>{f.name}</strong>
              <span className="cell-muted"> — {f.description}</span>
            </span>
          </li>
        ))}
      </ul>
      <MessageBar intent="info">Flows are created as drafts. Open each one, check the mappings, test it, then publish and switch it on.</MessageBar>
      {install.error && <ErrorBox message={errorMessage(install.error)} />}
    </Dialog>
  );
}
