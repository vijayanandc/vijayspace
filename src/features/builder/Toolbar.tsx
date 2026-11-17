// Builder top bar: back, inline flow name, publish state and save status, then undo/redo, Issues,
// Versions, Test, Publish and the On/Off switch.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { ArrowLeft, FlaskConical, History, ListChecks, Redo2, Rocket, Undo2 } from 'lucide-react';
import { useState } from 'react';
import { Link } from 'react-router-dom';

import { ApiError, errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { Issue } from '../../api/types';
import { Badge } from '../../components/Badge';
import { Button } from '../../components/Button';
import { Switch } from '../../components/Switch';
import { useToast } from '../../components/Toast';
import { useBuilder } from './BuilderContext';
import { TestFlowDialog, VersionsDialog } from './FlowDialogs';
import { SaveChip } from './SaveChip';
import type { SaveStatus } from './useAutosave';

interface Props {
  saveStatus: SaveStatus;
  saveError: string | null;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  issues: Issue[];
  issuesOpen: boolean;
  onToggleIssues: () => void;
  onIssues: (issues: Issue[]) => void;
}

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
    onSuccess: (f) => {
      toast.show('success', f.is_enabled ? 'The flow is on' : 'The flow is off');
      refresh();
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const errors = props.issues.filter((i) => i.severity === 'error').length;
  return (
    <div className="builder-bar">
      <Link to="/flows" className="icon-btn" aria-label="Back to flows" title="Back to flows">
        <ArrowLeft size={18} aria-hidden="true" />
      </Link>
      <input
        className="title-input builder-name"
        aria-label="Flow name"
        title="Rename flow"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={() => name.trim() && name.trim() !== flow.name && rename.mutate()}
        onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      />
      {flow.published_version ? (
        <Badge tone="info" dot>
          Published v{flow.published_version}
        </Badge>
      ) : (
        <Badge tone="muted" dot>
          Draft
        </Badge>
      )}
      {flow.has_unpublished_changes && flow.published_version && <Badge tone="warning">Unpublished changes</Badge>}
      <SaveChip status={props.saveStatus} error={props.saveError} onRetry={() => void flush()} />
      <div className="toolbar-spacer" />
      <div className="bar-group">
        <button type="button" className="icon-btn" aria-label="Undo" title="Undo (Ctrl+Z)" disabled={!props.canUndo} onClick={props.onUndo}>
          <Undo2 size={18} aria-hidden="true" />
        </button>
        <button type="button" className="icon-btn" aria-label="Redo" title="Redo (Ctrl+Shift+Z)" disabled={!props.canRedo} onClick={props.onRedo}>
          <Redo2 size={18} aria-hidden="true" />
        </button>
      </div>
      <span className="bar-divider" aria-hidden="true" />
      <Button variant="ghost" icon={ListChecks} onClick={props.onToggleIssues} title="Validation issues" aria-pressed={props.issuesOpen} className={props.issuesOpen ? 'is-on' : undefined}>
        Issues
        {props.issues.length > 0 && <span className={`count-badge${errors ? '' : ' count-badge-warning'}`}>{props.issues.length}</span>}
      </Button>
      <Button variant="ghost" icon={History} onClick={() => setVersions(true)} title="Published versions">
        Versions
      </Button>
      <span className="bar-divider" aria-hidden="true" />
      <Button icon={FlaskConical} onClick={() => setTesting(true)} title="Run the draft once with a test payload">
        Test
      </Button>
      <Button variant="primary" icon={Rocket} disabled={publish.isPending} onClick={() => publish.mutate()}>
        {publish.isPending ? 'Publishing…' : 'Publish'}
      </Button>
      <span className="bar-divider" aria-hidden="true" />
      <Switch
        checked={flow.is_enabled}
        disabled={toggle.isPending || !flow.published_version}
        onChange={() => toggle.mutate()}
        label="Flow enabled"
        title={flow.published_version ? (flow.is_enabled ? 'Switch the flow off' : 'Switch the flow on') : 'Publish before enabling'}
      />
      <TestFlowDialog open={testing} onClose={() => setTesting(false)} />
      <VersionsDialog open={versions} onClose={() => setVersions(false)} />
    </div>
  );
}
