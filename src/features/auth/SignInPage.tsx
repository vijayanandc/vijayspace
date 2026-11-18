// The sign-in page (shown when signed out): Weft's brand panel and one button that continues to Logto, where people
// sign in with the same account they use for the Appblocks apps.

import { ArrowRight, Blocks, History, ShieldCheck, Workflow } from 'lucide-react';

import { Button } from '../../components/Button';
import { WeftLogo } from '../../components/WeftLogo';

const POINTS = [
  { icon: Workflow, text: 'Build flows with a drag-and-drop builder' },
  { icon: Blocks, text: 'Connect Zoho CRM, Appblocks FSM and HMS, and more' },
  { icon: History, text: 'See every run, step by step' },
];

export function SignInPage({ onSignIn, busy = false, error }: { onSignIn: () => void; busy?: boolean; error?: string }) {
  return (
    <div className="auth-page">
      <section className="auth-brand" aria-hidden="true">
        <div className="auth-brand-inner">
          <div className="auth-brand-mark">
            <WeftLogo size={36} />
            <span>Weft</span>
          </div>
          <h2>Automate the work between your apps.</h2>
          <ul>
            {POINTS.map((p) => (
              <li key={p.text}>
                <p.icon size={18} />
                {p.text}
              </li>
            ))}
          </ul>
        </div>
      </section>
      <main className="auth-panel">
        <div className="auth-card">
          <WeftLogo size={32} />
          <h1>Sign in to Weft</h1>
          <p className="muted">Use your Appblocks account. One sign-in works across Weft and the Appblocks apps.</p>
          {error && (
            <p className="auth-error" role="alert">
              {error}
            </p>
          )}
          <Button variant="primary" onClick={onSignIn} disabled={busy} className="auth-cta">
            {busy ? 'Opening sign-in…' : 'Sign in'}
            <ArrowRight size={16} aria-hidden="true" />
          </Button>
          <p className="auth-foot">
            <ShieldCheck size={14} aria-hidden="true" /> Weft never sees your password.
          </p>
        </div>
      </main>
    </div>
  );
}
