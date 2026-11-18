// Builder dialogs: test the draft with a payload, and the list of published versions.

import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import type { JsonValue } from '../../api/types';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { Dialog } from '../../components/Dialog';
import { EmptyState, ErrorBox, Loading } from '../../components/Empty';
import { MessageBar } from '../../components/MessageBar';
import { formatTime, relativeTime } from '../../lib/format';
import { useBuilder } from './BuilderContext';
import { JsonInput } from '../../forms/JsonInput';

export function TestFlowDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { flowId, flow, flush } = useBuilder();
  const navigate = useNavigate();
  const sample = flow.trigger?.sample;
  const [payload, setPayload] = useState<JsonValue>(sample ?? { body: {} });
  // Each opening starts from the latest captured sample (e.g. one just captured with "Listen for sample").
  const [opened, setOpened] = useState(open);
  if (open !== opened) {
    setOpened(open);
    if (open) setPayload(sample ?? { body: {} });
  }
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
      subtitle="Runs the current draft once with this trigger payload."
      open={open}
      onClose={onClose}
      wide
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={run.isPending} onClick={() => run.mutate()}>
            {run.isPending ? 'Starting…' : 'Run test'}
          </Button>
        </>
      }
    >
      <MessageBar intent="warning">Actions really call the connected apps.</MessageBar>
      <label className="field-label" htmlFor="test-payload">
        Trigger payload (JSON)
      </label>
      <JsonInput id="test-payload" value={payload} onChange={setPayload} rows={14} />
      {run.error && <ErrorBox message={errorMessage(run.error)} />}
    </Dialog>
  );
}

export function VersionsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { flowId, flow } = useBuilder();
  const versions = useQuery({ queryKey: ['versions', flowId], queryFn: () => endpoints.versions(flowId), enabled: open });
  return (
    <Dialog
      title="Published versions"
      subtitle="Each run is pinned to the version that was live when it started."
      open={open}
      onClose={onClose}
      footer={<Button onClick={onClose}>Close</Button>}
    >
      {versions.isLoading && <Loading />}
      {versions.error && <ErrorBox message={errorMessage(versions.error)} />}
      {versions.data?.length === 0 && (
        <EmptyState title="Not published yet" compact>
          Publish the flow to create version 1.
        </EmptyState>
      )}
      {versions.data && versions.data.length > 0 && (
        <table className="table table-plain">
          <thead>
            <tr>
              <th>Version</th>
              <th>Published</th>
              <th>
                <span className="sr-only">State</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {versions.data.map((v) => (
              <tr key={v.id}>
                <td className="row-title">v{v.version}</td>
                <td title={formatTime(v.created_at)}>
                  {formatTime(v.created_at)} <span className="cell-muted">· {relativeTime(v.created_at)}</span>
                </td>
                <td>{v.version === flow.published_version && <Badge tone="success" dot>Live</Badge>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Dialog>
  );
}
