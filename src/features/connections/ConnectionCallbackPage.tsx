// Landing page after an OAuth redirect: `?status=connected&connection_id=…` or `?status=error&message=…`.

import { useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import { Link, useSearchParams } from 'react-router-dom';

import { qk } from '../../api/hooks';

export function ConnectionCallbackPage() {
  const [params] = useSearchParams();
  const qc = useQueryClient();
  const status = params.get('status');
  const id = params.get('connection_id');
  const message = params.get('message');
  useEffect(() => {
    void qc.invalidateQueries({ queryKey: qk.connections });
  }, [qc]);
  return (
    <div className="page narrow">
      {status === 'connected' ? (
        <>
          <h1>Connected ✓</h1>
          <p>The connection {id && <span className="mono">{id}</span>} is ready to use in flows.</p>
        </>
      ) : (
        <>
          <h1>Connection failed</h1>
          <p className="error-box" role="alert">{message ?? 'The app did not grant access.'}</p>
        </>
      )}
      <Link to="/connections">← Back to connections</Link>
    </div>
  );
}
