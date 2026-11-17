// Landing page after an OAuth redirect: `?status=connected&connection_id=…` or `?status=error&message=…`.
// With `next` (an OAuth started from the builder), a success goes straight back to that page.

import { useQueryClient } from '@tanstack/react-query';
import { CircleCheck, CircleX } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { qk } from '../../api/hooks';
import { LinkButton } from '../../components/Button';
import { useToast } from '../../components/Toast';
import { safeNextPath } from './oauth';

export function ConnectionCallbackPage() {
  const [params] = useSearchParams();
  const qc = useQueryClient();
  const toast = useToast();
  const navigate = useNavigate();
  const status = params.get('status');
  const id = params.get('connection_id');
  const message = params.get('message');
  const next = safeNextPath(params.get('next'));
  const ok = status === 'connected';
  const handled = useRef(false);
  useEffect(() => {
    if (handled.current) return;
    handled.current = true;
    void qc.invalidateQueries({ queryKey: qk.connections });
    if (ok && next) {
      toast.show('success', 'Connected. Choose the new connection in the step.');
      navigate(next, { replace: true });
    }
  }, [qc, ok, next, toast, navigate]);
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
