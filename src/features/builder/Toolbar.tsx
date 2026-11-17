import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Link } from 'react-router-dom';

import { ApiError, errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Issue } from '../../api/types';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { useToast } from '../../components/Toast';
import { useBuilder } from './BuilderContext';
import { TestFlowDialog, VersionsDialog } from './FlowDialogs';
import type { SaveStatus } from './useAutosave';

interface Props {
  saveStatus: SaveStatus;
  saveError: string | null;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  issueCount: number;
  onToggleIssues: () => void;
  onIssues: (issues: Issue[]) => void;
}

const STATUS_TEXT: Record<SaveStatus, string> = {
  saved: 'All changes saved',
  pending: 'Unsaved changes…',
  saving: 'Saving…',
  error: 'Save failed',
  conflict: 'Conflict',
};

export function Toolbar(props: Props) {
  const { flowId, flow, flush, refetchFlow } = useBuilder();
  const qc = useQueryClient();
  const toast = useToast();
  const [name, setName] = useState(flow.name);
  const [testing, setTesting] = useState(false);
  const [versions, setVersions] = useState(false);
  const refresh = () => {
    refetchFlow();
    void qc.invalidateQueries({ queryKey: qk.flows });
  };
  const rename = useMutation({ mutationFn: () => endpoints.updateFlow(flowId, { name: name.trim() }), onSuccess: refresh, onError: (e) => toast.show('error', errorMessage(e)) });
  const publish = useMutation({
    mutationFn: async () => {
      if (!(await flush())) throw new Error('Save the draft first (see the save status).');
      return endpoints.publish(flowId);
    },
    onSuccess: (res) => {
      toast.show('success', `Published v${res.version.version}`);
      props.onIssues(res.issues);
      refresh();
    },
    onError: (e) => {
      if (e instanceof ApiError && e.issues.length > 0) props.onIssues(e.issues);
      toast.show('error', errorMessage(e));
    },
  });
  const toggle = useMutation({
    mutationFn: () => (flow.is_enabled ? endpoints.disable(flowId) : endpoints.enable(flowId)),
    onSuccess: refresh,
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  return (
    <div className="toolbar">
      <Link to="/flows" className="muted">
        ← Flows
      </Link>
      <input
        className="input toolbar-name"
        aria-label="Flow name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => name.trim() && name.trim() !== flow.name && rename.mutate()}
      />
      <span className={`save-status save-${props.saveStatus}`} title={props.saveError ?? undefined}>
        {STATUS_TEXT[props.saveStatus]}
      </span>
      {props.saveStatus === 'error' && (
        <Button variant="ghost" onClick={() => void flush()}>
          Retry save
        </Button>
      )}
      <div className="toolbar-spacer" />
      <Button variant="ghost" aria-label="Undo" title="Undo (Ctrl+Z)" disabled={!props.canUndo} onClick={props.onUndo}>
        ↶
      </Button>
      <Button variant="ghost" aria-label="Redo" title="Redo (Ctrl+Shift+Z)" disabled={!props.canRedo} onClick={props.onRedo}>
        ↷
      </Button>
      <Button variant="ghost" onClick={props.onToggleIssues}>
        Issues {props.issueCount > 0 && <Badge tone="warning">{props.issueCount}</Badge>}
      </Button>
      <Button variant="ghost" onClick={() => setVersions(true)}>
        {flow.published_version ? `v${flow.published_version}` : 'Versions'}
      </Button>
      {flow.has_unpublished_changes && flow.published_version && <Badge tone="warning">unpublished changes</Badge>}
      <Button onClick={() => setTesting(true)}>Test</Button>
      <Button variant="primary" disabled={publish.isPending} onClick={() => publish.mutate()}>
        {publish.isPending ? 'Publishing…' : 'Publish'}
      </Button>
      <label className="switch" title={flow.published_version ? undefined : 'Publish before enabling'}>
        <input type="checkbox" checked={flow.is_enabled} disabled={toggle.isPending || !flow.published_version} onChange={() => toggle.mutate()} aria-label="Flow enabled" />
        <span className="switch-track" />
        <span className="switch-label">{flow.is_enabled ? 'On' : 'Off'}</span>
      </label>
      <TestFlowDialog open={testing} onClose={() => setTesting(false)} />
      <VersionsDialog open={versions} onClose={() => setVersions(false)} />
    </div>
  );
}
