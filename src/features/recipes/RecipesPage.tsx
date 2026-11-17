import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnectors } from '../../api/hooks';
import type { Recipe } from '../../api/types';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { InstallWizard } from './InstallWizard';

export function RecipesPage() {
  const recipes = useQuery({ queryKey: qk.recipes, queryFn: endpoints.recipes });
  const { data: manifests = [] } = useConnectors();
  const [installing, setInstalling] = useState<Recipe | null>(null);
  const appName = (key: string) => {
    const m = manifests.find((x) => x.key === key);
    return m ? `${m.icon} ${m.name}` : key;
  };
  return (
    <div className="page">
      <div className="page-header">
        <h1>Recipes</h1>
      </div>
      <p className="muted">Prebuilt integrations between Zoho CRM, Appblocks apps (FSM, HMS) and Autodesk Forma. Installing creates draft flows you can review before publishing.</p>
      {recipes.isLoading && <Loading />}
      {recipes.error && <ErrorBox message={errorMessage(recipes.error)} />}
      {recipes.data?.length === 0 && <EmptyState title="No recipes available" />}
      <div className="cards">
        {recipes.data?.map((r) => (
          <article key={r.key} className="card">
            <h3>{r.name}</h3>
            <div className="card-apps">
              {r.apps.map((a) => (
                <Badge key={a} tone="info">
                  {appName(a)}
                </Badge>
              ))}
            </div>
            <p>{r.description}</p>
            <ul className="small">
              {r.flows.map((f) => (
                <li key={f.name}>
                  <strong>{f.name}</strong> — {f.description}
                </li>
              ))}
            </ul>
            <Button variant="primary" onClick={() => setInstalling(r)}>
              Install
            </Button>
          </article>
        ))}
      </div>
      {installing && <InstallWizard recipe={installing} onClose={() => setInstalling(null)} />}
    </div>
  );
}
