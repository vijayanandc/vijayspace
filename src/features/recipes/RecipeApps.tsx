// App tiles joined by arrows, e.g. Zoho CRM → Appblocks (recipe cards and the install wizard).

import { ArrowRight } from 'lucide-react';
import { Fragment } from 'react';

import { AppIcon } from '../../components/AppIcon';

export function RecipeApps({ apps, name, size = 36 }: { apps: string[]; name: (key: string) => string; size?: number }) {
  return (
    <div className="recipe-apps">
      {apps.map((a, i) => (
        <Fragment key={a}>
          {i > 0 && <ArrowRight size={16} className="recipe-arrow" aria-hidden="true" />}
          <AppIcon app={a} name={name(a)} size={size} />
        </Fragment>
      ))}
    </div>
  );
}
