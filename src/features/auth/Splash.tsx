// Full-page states before the app shell exists: loading, and errors with a way forward.

import type { ReactNode } from 'react';

import { WeftLogo } from '../../components/WeftLogo';

export function Splash({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="auth-splash" role="status" aria-live="polite">
      <WeftLogo size={40} />
      <p className="auth-splash-title">{title}</p>
      {children}
    </div>
  );
}
