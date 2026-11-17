// Global header: app launcher, wordmark, global search, help, settings, org and avatar.

import { CircleHelp, Grip, Settings } from 'lucide-react';
import { Link } from 'react-router-dom';

import { useOrg } from '../../api/hooks';
import { Popover } from '../../components/Popover';
import { WeftLogo } from '../../components/WeftLogo';
import { GlobalSearch } from './GlobalSearch';
import { NAV, initials } from './nav';

function AppLauncher() {
  return (
    <Popover
      align="left"
      className="launcher"
      trigger={(open, toggle) => (
        <button type="button" className="header-btn" aria-label="App launcher" title="App launcher" aria-expanded={open} onClick={toggle}>
          <Grip size={20} aria-hidden="true" />
        </button>
      )}
    >
      {(close) => (
        <nav aria-label="App launcher">
          <div className="popover-title">Weft</div>
          <div className="launcher-grid">
            {NAV.map((item) => (
              <Link key={item.to} to={item.to} className="launcher-item" onClick={close}>
                <span className="launcher-tile" style={{ background: item.tone }} aria-hidden="true">
                  <item.icon size={18} />
                </span>
                <span>
                  <span className="launcher-name">{item.label}</span>
                  <span className="launcher-desc">{item.description}</span>
                </span>
              </Link>
            ))}
          </div>
        </nav>
      )}
    </Popover>
  );
}

const SHORTCUTS: [string, string][] = [
  ['Undo', 'Ctrl+Z'],
  ['Redo', 'Ctrl+Shift+Z'],
  ['Delete selected step', 'Delete'],
  ['Deselect / cancel insert', 'Esc'],
];

function HelpMenu() {
  return (
    <Popover
      trigger={(open, toggle) => (
        <button type="button" className="header-btn" aria-label="Help" title="Help" aria-expanded={open} onClick={toggle}>
          <CircleHelp size={20} aria-hidden="true" />
        </button>
      )}
    >
      {() => (
        <div className="help-pop">
          <div className="popover-title">Builder shortcuts</div>
          <dl className="kbd-list">
            {SHORTCUTS.map(([what, keys]) => (
              <div key={what} className="kbd-row">
                <dt>{what}</dt>
                <dd>
                  <kbd>{keys}</kbd>
                </dd>
              </div>
            ))}
          </dl>
          <p className="popover-note">Drag steps from the palette onto a + on the canvas. Drag a step onto another + to move it.</p>
        </div>
      )}
    </Popover>
  );
}

function SettingsMenu() {
  const { data: org } = useOrg();
  return (
    <Popover
      trigger={(open, toggle) => (
        <button type="button" className="header-btn" aria-label="Settings" title="Settings" aria-expanded={open} onClick={toggle}>
          <Settings size={20} aria-hidden="true" />
        </button>
      )}
    >
      {(close) => (
        <div className="help-pop">
          <div className="popover-title">Organization</div>
          <dl className="kbd-list">
            <div className="kbd-row">
              <dt>Name</dt>
              <dd>{org?.name ?? '—'}</dd>
            </div>
            <div className="kbd-row">
              <dt>Time zone</dt>
              <dd>{org?.timezone ?? '—'}</dd>
            </div>
          </dl>
          {org && <div className="popover-note mono">{org.id}</div>}
          <Link to="/connections" className="menu-item" onClick={close}>
            Manage connections
          </Link>
        </div>
      )}
    </Popover>
  );
}

export function GlobalHeader() {
  const { data: org } = useOrg();
  const orgName = org?.name ?? 'Workspace';
  return (
    <header className="app-header" role="banner">
      <div className="header-left">
        <AppLauncher />
        <Link to="/flows" className="brand" aria-label="Weft home">
          <WeftLogo size={24} />
          <span className="brand-name">Weft</span>
        </Link>
      </div>
      <GlobalSearch />
      <div className="header-right">
        <HelpMenu />
        <SettingsMenu />
        <span className="header-org" title={org?.timezone ? `${orgName} · ${org.timezone}` : orgName}>
          {orgName}
        </span>
        <span className="avatar" aria-hidden="true">
          {initials(orgName)}
        </span>
      </div>
    </header>
  );
}
