// Collapsible left navigation rail (Fluent nav): icons + labels, active indicator bar, usage summary.

import { useQuery } from '@tanstack/react-query';
import { Menu } from 'lucide-react';
import { NavLink } from 'react-router-dom';

import { endpoints } from '../../api/endpoints';
import { NAV } from './nav';

function UsageCard() {
  const usage = useQuery({ queryKey: ['usage'], queryFn: endpoints.usage, staleTime: 60_000, retry: false });
  if (!usage.data) return null;
  const { tasks_used, executions, period } = usage.data;
  const [year, month] = period.split('-').map(Number);
  const label = year && month ? new Date(Date.UTC(year, month - 1, 1)).toLocaleString(undefined, { month: 'long', year: 'numeric', timeZone: 'UTC' }) : period;
  return (
    <div className="nav-usage">
      <div className="nav-usage-title">Usage · {label}</div>
      <div className="nav-usage-row">
        <span>
          <strong>{tasks_used}</strong> tasks
        </span>
        <span>
          <strong>{executions}</strong> runs
        </span>
      </div>
    </div>
  );
}

export function NavRail({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  return (
    <nav className={`nav-rail${collapsed ? ' nav-collapsed' : ''}`} aria-label="Main">
      <button
        type="button"
        className="nav-link nav-toggle"
        aria-label={collapsed ? 'Expand navigation' : 'Collapse navigation'}
        title={collapsed ? 'Expand navigation' : 'Collapse navigation'}
        aria-expanded={!collapsed}
        onClick={onToggle}
      >
        <Menu size={20} aria-hidden="true" />
      </button>
      {NAV.map((item) => (
        <NavLink key={item.to} to={item.to} className={({ isActive }) => `nav-link${isActive ? ' nav-active' : ''}`} title={collapsed ? item.label : undefined}>
          <item.icon size={20} aria-hidden="true" />
          <span className="nav-label">{item.label}</span>
        </NavLink>
      ))}
      <div className="nav-spacer" />
      {!collapsed && <UsageCard />}
    </nav>
  );
}
