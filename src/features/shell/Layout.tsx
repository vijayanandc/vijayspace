// App shell: global header, collapsible left rail (hidden in the builder) and the routed page.

import { Outlet, useMatch } from 'react-router-dom';

import { useStoredChoice } from '../../lib/storage';
import { GlobalHeader } from './GlobalHeader';
import { NavRail } from './NavRail';

const NAV_STATES = ['expanded', 'collapsed'] as const;

export function Layout() {
  const inBuilder = useMatch('/flows/:flowId') !== null;
  const [nav, setNav] = useStoredChoice('weft.nav', 'expanded', NAV_STATES);
  return (
    <div className="app">
      <GlobalHeader />
      <div className="app-body">
        {!inBuilder && <NavRail collapsed={nav === 'collapsed'} onToggle={() => setNav(nav === 'collapsed' ? 'expanded' : 'collapsed')} />}
        <main className={`content${inBuilder ? ' content-builder' : ''}`}>
          <Outlet />
        </main>
      </div>
    </div>
  );
}
