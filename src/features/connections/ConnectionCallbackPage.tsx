// Landing page after an OAuth redirect: `?status=connected&connection_id=…` or `?status=error&message=…`.

import { useQueryClient } from '@tanstack/react-query';
import { CircleCheck, CircleX } from 'lucide-react';
import { useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';

import { qk } from '../../api/hooks';
import { LinkButton } from '../../components/Button';

export function ConnectionCallbackPage() {
  const [params] = useSearchParams();
  const qc = useQueryClient();
  const status = params.get('status');
  const id = params.get('connection_id');
  const message = params.get('message');
  useEffect(() => {
    void qc.invalidateQueries({ queryKey: qk.connections });
  }, [qc]);
  const ok = status === 'connected';
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
        <LinkButton to="/connections" variant="primary">
          Back to connections
        </LinkButton>
      </section>
    </div>
  );
}
