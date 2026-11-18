// Landing page after an OAuth redirect. `?status=finish&state=…&finish=…`: the provider answered, and this signed-in
// page finishes the sign-in (once) in the org it was started in (`org`). Then `?status=connected&connection_id=…`
// or `?status=error&message=…`. With `next` (an OAuth started from the builder), a success goes straight back there.

import { useQueryClient } from '@tanstack/react-query';
import { CircleCheck, CircleX } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import { LinkButton } from '../../components/Button';
import { useToast } from '../../components/Toast';
import { useSession } from '../auth/session';
import { OAUTH_RETURN_PATH, safeNextPath } from './oauth';

/** The callback URL that shows `result` (the finish token never stays in the address bar or history). */
function resultUrl(result: Record<string, string>, next: string | null): string {
  const query = new URLSearchParams(result);
  if (next) query.set('next', next);
  return `${OAUTH_RETURN_PATH}?${query.toString()}`;
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
    if (!state || !finish) {
      navigate(resultUrl({ status: 'error', message: 'The app sent the browser back without a sign-in to finish.' }, next), { replace: true });
      return;
    }
    endpoints
      .oauthFinish(state, finish, org)
      .then((res) => navigate(resultUrl({ status: 'connected', connection_id: res.connection_id }, next), { replace: true }))
      .catch((e: unknown) => navigate(resultUrl({ status: 'error', message: errorMessage(e) }, next), { replace: true }));
  }, [active, state, finish, org, next, navigate]);
  return active;
}

export function ConnectionCallbackPage() {
  const [params] = useSearchParams();
  const qc = useQueryClient();
  const toast = useToast();
  const navigate = useNavigate();
  const status = params.get('status');
  const id = params.get('connection_id');
  const message = params.get('message');
  const next = safeNextPath(params.get('next'));
  const finishing = useFinish(params, next);
  const ok = status === 'connected';
  const handled = useRef(false);
  useEffect(() => {
    if (finishing || handled.current) return;
    handled.current = true;
    void qc.invalidateQueries({ queryKey: qk.connections });
    if (ok && next) {
      toast.show('success', 'Connected. Choose the new connection in the step.');
      navigate(next, { replace: true });
    }
  }, [finishing, qc, ok, next, toast, navigate]);
  if (finishing) {
    return (
      <div className="page">
        <section className="result-card" aria-busy="true">
          <h1>Finishing the connection…</h1>
        </section>
      </div>
    );
  }
  return (
    <div className="page">
      <section className={`result-card ${ok ? 'result-ok' : 'result-error'}`}>
        {ok ? <CircleCheck size={40} aria-hidden="true" /> : <CircleX size={40} aria-hidden="true" />}
        <h1>{ok ? 'Connected' : 'Connection failed'}</h1>
        {ok ? (
          <p>The connection {id && <span className="mono">{id}</span>} is ready to use in flows.</p>
        ) : (
          <p role="alert">{message ?? 'The app did not grant access.'}</p>
        )}
        <LinkButton to={next ?? '/connections'} variant="primary">
          {next ? 'Back to the flow' : 'Back to connections'}
        </LinkButton>
      </section>
    </div>
  );
}
