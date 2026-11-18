// Recipe gallery (Zoho Flow "Gallery"): filter by app, search, and cards showing the apps a recipe joins.

import { useQuery } from '@tanstack/react-query';
import { LayoutGrid, Workflow } from 'lucide-react';
import { useState } from 'react';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk, useConnectors } from '../../api/hooks';
import type { Recipe } from '../../api/types';
import { AppIcon } from '../../components/AppIcon';
import { Button } from '../../components/Button';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { PageHeader } from '../../components/PageHeader';
import { SearchBox } from '../../components/SearchBox';
import { InstallWizard } from './InstallWizard';
import { RecipeApps } from './RecipeApps';
import { can } from '../auth/permissions';
import { useRole } from '../auth/session';

export function RecipesPage() {
  const role = useRole();
  const canInstall = can(role, 'flow.edit') && can(role, 'connection.use');
  const recipes = useQuery({ queryKey: qk.recipes, queryFn: endpoints.recipes });
  const { data: manifests = [] } = useConnectors();
  const [installing, setInstalling] = useState<Recipe | null>(null);
  const [app, setApp] = useState('');
  const [query, setQuery] = useState('');
  const name = (key: string) => manifests.find((x) => x.key === key)?.name ?? key;
  const all = recipes.data ?? [];
  const apps = [...new Set(all.flatMap((r) => r.apps))];
  const q = query.trim().toLowerCase();
  const visible = all.filter((r) => (!app || r.apps.includes(app)) && (!q || r.name.toLowerCase().includes(q) || r.description.toLowerCase().includes(q)));

  return (
    <div className="page">
      <section className="list-card">
        <PageHeader
          icon={LayoutGrid}
          tone="#8764b8"
          label="Gallery"
          title="Recipe gallery"
          meta={`${all.length} prebuilt integrations · Installing creates draft flows you review before publishing`}
        />
        <div className="command-bar">
          <SearchBox label="Search recipes" placeholder="Search recipes" value={query} onChange={setQuery} className="command-search" />
          <div className="chip-group" role="group" aria-label="Filter by app">
            <button type="button" className={`chip${app === '' ? ' chip-active' : ''}`} aria-pressed={app === ''} onClick={() => setApp('')}>
              All apps
            </button>
            {apps.map((a) => (
              <button key={a} type="button" className={`chip${app === a ? ' chip-active' : ''}`} aria-pressed={app === a} onClick={() => setApp(a)}>
                <AppIcon app={a} size={16} decorative />
                {name(a)}
              </button>
            ))}
          </div>
        </div>
      </section>
      {recipes.isLoading && <Loading />}
      {recipes.error && <ErrorBox message={errorMessage(recipes.error)} />}
      {recipes.data && visible.length === 0 && (
        <div className="list-card">
          <EmptyState title={all.length ? 'No recipes match' : 'No recipes available'} compact={all.length > 0}>
            {all.length ? 'Try another app or search term.' : 'Recipes appear here when the server provides them.'}
          </EmptyState>
        </div>
      )}
      <div className="gallery-grid">
        {visible.map((r) => (
          <article key={r.key} className="recipe-card">
            <RecipeApps apps={r.apps} name={name} />
            <div className="recipe-app-names">{r.apps.map(name).join(' → ')}</div>
            <h3 className="recipe-title">{r.name}</h3>
            <p className="recipe-desc" title={r.description}>
              {r.description}
            </p>
            <div className="recipe-foot">
              <span className="recipe-count">
                <Workflow size={14} aria-hidden="true" />
                {r.flows.length} flow{r.flows.length === 1 ? '' : 's'}
              </span>
              {canInstall && (
                <Button variant="primary" size="sm" onClick={() => setInstalling(r)}>
                  Install
                </Button>
              )}
            </div>
          </article>
        ))}
      </div>
      {installing && <InstallWizard recipe={installing} name={name} onClose={() => setInstalling(null)} />}
    </div>
  );
}
