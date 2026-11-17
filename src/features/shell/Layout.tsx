import { NavLink, Outlet } from 'react-router-dom';

import { useOrg } from '../../api/hooks';

const NAV = [
  { to: '/flows', label: 'Flows', icon: '⇄' },
  { to: '/recipes', label: 'Recipes', icon: '✦' },
  { to: '/connections', label: 'Connections', icon: '🔗' },
  { to: '/history', label: 'History', icon: '☰' },
];

export function Layout() {
  const { data: org } = useOrg();
  return (
    <div className="app">
      <nav className="sidebar" aria-label="Main">
        <div className="brand">
          <span className="brand-mark">W</span> Weft
        </div>
        {NAV.map((item) => (
          <NavLink key={item.to} to={item.to} className={({ isActive }) => `nav-link${isActive ? ' nav-active' : ''}`}>
            <span aria-hidden="true" className="nav-icon">{item.icon}</span>
            {item.label}
          </NavLink>
        ))}
      </nav>
      <div className="main">
        <header className="topbar">
          <span className="topbar-org">{org?.name ?? 'Workspace'}</span>
          {org?.timezone && <span className="muted"> · {org.timezone}</span>}
        </header>
        <main className="content">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
