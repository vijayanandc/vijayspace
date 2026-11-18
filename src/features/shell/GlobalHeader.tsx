// Global header: app launcher, wordmark, global search, help, settings, organisation switcher and profile menu.

import { CircleHelp, Grip, Settings } from 'lucide-react';
import { Link, NavLink } from 'react-router-dom';

import { Popover } from '../../components/Popover';
import { WeftLogo } from '../../components/WeftLogo';
import { OrgSwitcher, ProfileMenu } from './AccountMenus';
import { GlobalSearch } from './GlobalSearch';
import { LAUNCHER } from './nav';

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
            {LAUNCHER.map((item) => (
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

/** The gear opens the Settings page. */
function SettingsLink() {
  return (
    <NavLink to="/settings" className={({ isActive }) => `header-btn${isActive ? ' header-btn-active' : ''}`} aria-label="Settings" title="Settings">
      <Settings size={20} aria-hidden="true" />
    </NavLink>
  );
}

export function GlobalHeader() {
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
        <SettingsLink />
        <OrgSwitcher />
        <ProfileMenu />
      </div>
    </header>
  );
}
