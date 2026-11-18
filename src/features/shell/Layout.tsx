// App shell: global header, collapsible left rail (hidden in the builder) and the routed page (loaded on demand).

import { Suspense } from 'react';
import { Outlet, useLocation, useMatch } from 'react-router-dom';

import { Loading } from '../../components/Empty';
import { ErrorBoundary } from '../../components/ErrorBoundary';
import { useStoredChoice } from '../../lib/storage';
import { GlobalHeader } from './GlobalHeader';
import { NavRail } from './NavRail';

const NAV_STATES = ['expanded', 'collapsed'] as const;

export function Layout() {
  const inBuilder = useMatch('/flows/:flowId') !== null;
  const { pathname } = useLocation();
  const [nav, setNav] = useStoredChoice('weft.nav', 'expanded', NAV_STATES);
  return (
    <div className="app">
      <GlobalHeader />
      <div className="app-body">
        {!inBuilder && <NavRail collapsed={nav === 'collapsed'} onToggle={() => setNav(nav === 'collapsed' ? 'expanded' : 'collapsed')} />}
        <main className={`content${inBuilder ? ' content-builder' : ''}`}>
          <ErrorBoundary resetKey={pathname}>
            {/* Pages load on first visit (each is its own chunk); the header and rail stay. */}
            <Suspense fallback={<Loading label="Loading page…" />}>
              <Outlet />
            </Suspense>
          </ErrorBoundary>
        </main>
      </div>
    </div>
  );
}
