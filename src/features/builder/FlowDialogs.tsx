import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import type { JsonValue } from '../../api/types';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { ErrorBox, Loading } from '../../components/Empty';
import { formatTime } from '../../lib/format';
import { useBuilder } from './BuilderContext';
import { JsonInput } from './forms/JsonInput';

export function TestFlowDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { flowId, flow, flush } = useBuilder();
  const navigate = useNavigate();
  const [payload, setPayload] = useState<JsonValue>(flow.trigger?.sample ?? { body: {} });
  const run = useMutation({
    mutationFn: async () => {
      if (!(await flush())) throw new Error('The draft could not be saved; fix the save error first.');
      return endpoints.testFlow(flowId, payload);
    },
    onSuccess: (res) => navigate(`/history/${res.execution_id}`),
  });
  return (
    <Dialog
      title="Test the draft"
      open={open}
      onClose={onClose}
      wide
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={run.isPending} onClick={() => run.mutate()}>
            Run test
          </Button>
        </>
      }
    >
      <p className="muted small">Runs the current draft once with this trigger payload. Actions really call the connected apps.</p>
      <JsonInput id="test-payload" value={payload} onChange={setPayload} rows={14} />
      {run.error && <ErrorBox message={errorMessage(run.error)} />}
    </Dialog>
  );
}

export function VersionsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { flowId } = useBuilder();
  const versions = useQuery({ queryKey: ['versions', flowId], queryFn: () => endpoints.versions(flowId), enabled: open });
  return (
    <Dialog title="Published versions" open={open} onClose={onClose}>
      {versions.isLoading && <Loading />}
      {versions.error && <ErrorBox message={errorMessage(versions.error)} />}
      {versions.data?.length === 0 && <p className="muted">Not published yet.</p>}
      <ul className="plain-list">
        {versions.data?.map((v) => (
          <li key={v.id}>
            <strong>v{v.version}</strong> <span className="muted">{formatTime(v.created_at)}</span>
          </li>
        ))}
      </ul>
      <p className="muted small">Each run is pinned to the version that was live when it started.</p>
    </Dialog>
  );
}
