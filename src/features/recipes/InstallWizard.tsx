import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Recipe } from '../../api/types';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox } from '../../components/Empty';
import { ConnectionPicker } from '../builder/panel/ConnectionPicker';

/** Pick (or create) one connection per recipe slot, then install the recipe's flows as drafts. */
export function InstallWizard({ recipe, onClose }: { recipe: Recipe; onClose: () => void }) {
  const [chosen, setChosen] = useState<Record<string, string>>({});
  const navigate = useNavigate();
  const qc = useQueryClient();
  const complete = recipe.slots.every((s) => Boolean(chosen[s.slot]));
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
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!complete || install.isPending} onClick={() => install.mutate()}>
            Install {recipe.flows.length} flow{recipe.flows.length === 1 ? '' : 's'}
          </Button>
        </>
      }
    >
      <p className="muted">{recipe.description}</p>
      <ol className="wizard-steps">
        {recipe.slots.map((slot) => (
          <li key={slot.slot}>
            <strong>{slot.label}</strong>
            <ConnectionPicker connector={slot.connector} value={chosen[slot.slot] ?? null} onChange={(id) => setChosen((prev) => ({ ...prev, [slot.slot]: id ?? '' }))} />
          </li>
        ))}
      </ol>
      <p className="muted small">Flows are created as drafts. Open each one, check the mappings, test it, then publish and switch it on.</p>
      {install.error && <ErrorBox message={errorMessage(install.error)} />}
    </Dialog>
  );
}
