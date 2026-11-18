// Landing page after an OAuth redirect. `?status=finish&state=…&finish=…`: the provider answered, and this signed-in
// page finishes the sign-in (once) in the org it was started in (`org`); the outcome is shown from router state, so
// a crafted link cannot show "Connected" or its own text. `?status=error&message=<code>`: the provider refused. With
// `next` (an OAuth started from the builder), a success goes straight back there.

import { useQueryClient } from '@tanstack/react-query';
import { CircleCheck, CircleX } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import { LinkButton } from '../../components/Button';
import { useToast } from '../../components/Toast';
import { useSession } from '../auth';
import { safeNextPath } from '../../lib/safePath';
import { OAUTH_RETURN_PATH } from './oauth';

/** The outcome of a finished sign-in, carried in router state: a URL alone can never claim one. */
type Result = { status: 'connected'; connectionId: string } | { status: 'error'; message: string };

function resultOf(state: unknown): Result | null {
  if (typeof state !== 'object' || state === null || !('result' in state)) return null;
  const r = (state as { result: unknown }).result;
  if (typeof r !== 'object' || r === null) return null;
  const v = r as Record<string, unknown>;
  if (v.status === 'connected' && typeof v.connectionId === 'string') return { status: 'connected', connectionId: v.connectionId };
  if (v.status === 'error' && typeof v.message === 'string') return { status: 'error', message: v.message };
  return null;
}

/** The page again, without the finish token (never kept in the address bar or history). */
function landing(next: string | null): string {
  return next ? `${OAUTH_RETURN_PATH}?next=${encodeURIComponent(next)}` : OAUTH_RETURN_PATH;
}

/** A provider's error code (`access_denied`…) as sent back by the server; anything else is not shown. */
function providerCode(message: string | null): string | null {
  return message && /^[A-Za-z0-9_.-]{1,64}$/.test(message) ? message : null;
}

/** Finishes a parked sign-in once (the server spends it on the first try, so StrictMode must not repeat it). */
function useFinish(params: URLSearchParams, next: string | null) {
  const navigate = useNavigate();
  const session = useSession();
  const started = useRef(false);
  const state = params.get('state');
  const finish = params.get('finish');
  const wanted = params.get('org');
  // Only an org the user belongs to; the server checks it again.
  const org = session?.me.orgs.some((o) => o.id === wanted) ? (wanted ?? undefined) : undefined;
  const active = params.get('status') === 'finish';
  useEffect(() => {
    if (!active || started.current) return;
    started.current = true;
    const done = (result: Result) => navigate(landing(next), { replace: true, state: { result } });
    if (!state || !finish) {
      done({ status: 'error', message: 'The app sent the browser back without a sign-in to finish.' });
      return;
    }
    endpoints
      .oauthFinish(state, finish, org)
      .then((res) => done({ status: 'connected', connectionId: res.connection_id }))
      .catch((e: unknown) => done({ status: 'error', message: errorMessage(e) }));
  }, [active, state, finish, org, next, navigate]);
  return active;
}

export function ConnectionCallbackPage() {
  const [params] = useSearchParams();
  const location = useLocation();
  const qc = useQueryClient();
  const toast = useToast();
  const navigate = useNavigate();
  const next = safeNextPath(params.get('next'));
  const finishing = useFinish(params, next);
  const result = resultOf(location.state);
  const ok = result?.status === 'connected';
  const providerError = params.get('status') === 'error';
  const handled = useRef(false);
  useEffect(() => {
    if (finishing || handled.current || !result) return;
    handled.current = true;
    void qc.invalidateQueries({ queryKey: qk.connections });
    if (ok && next) {
      toast.show('success', 'Connected. Choose the new connection in the step.');
      navigate(next, { replace: true });
    }
  }, [finishing, result, qc, ok, next, toast, navigate]);
  if (finishing) {
    return (
      <div className="page">
        <section className="result-card" aria-busy="true">
          <h1>Finishing the connection…</h1>
        </section>
      </div>
    );
  }
  const code = providerCode(params.get('message'));
  const failure = result?.status === 'error' ? result.message : providerError ? `The app did not grant access${code ? ` (${code})` : ''}.` : null;
  return (
    <div className="page">
      <section className={`result-card ${ok ? 'result-ok' : 'result-error'}`}>
        {ok ? <CircleCheck size={40} aria-hidden="true" /> : <CircleX size={40} aria-hidden="true" />}
        <h1>{ok ? 'Connected' : failure ? 'Connection failed' : 'Nothing to finish here'}</h1>
        {ok && result?.status === 'connected' && (
          <p>
            The connection <span className="mono">{result.connectionId}</span> is ready to use in flows.
          </p>
        )}
        {!ok && <p role={failure ? 'alert' : undefined}>{failure ?? 'This page completes an app sign-in started from Weft.'}</p>}
        <LinkButton to={next ?? '/connections'} variant="primary">
          {next ? 'Back to the flow' : 'Back to connections'}
        </LinkButton>
      </section>
    </div>
  );
}
