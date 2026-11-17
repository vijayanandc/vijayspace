// Row actions for a flow in the list: enable/disable, run now, delete, and the overflow menu items.

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { History, Pencil, Play, Trash2 } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { errorMessage } from '../../api/client';
import { endpoints } from '../../api/endpoints';
import { qk } from '../../api/hooks';
import type { FlowSummary } from '../../api/types';
import type { MenuItem } from '../../components/Popover';
import { useToast } from '../../components/Toast';

export function useFlowActions(flow: FlowSummary) {
  const qc = useQueryClient();
  const toast = useToast();
  const navigate = useNavigate();
  const refresh = () => void qc.invalidateQueries({ queryKey: qk.flows });
  const toggle = useMutation({
    mutationFn: () => (flow.is_enabled ? endpoints.disable(flow.id) : endpoints.enable(flow.id)),
    onSuccess: (f) => {
      toast.show('success', `“${flow.name}” is ${f.is_enabled ? 'on' : 'off'}`);
      refresh();
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: () => endpoints.deleteFlow(flow.id),
    onSuccess: () => {
      toast.show('success', `Deleted “${flow.name}”`);
      refresh();
    },
    onError: (e) => toast.show('error', errorMessage(e)),
  });
  const run = useMutation({
    mutationFn: () => endpoints.runFlow(flow.id, {}),
    onSuccess: (res) => navigate(`/history/${res.execution_id}`),
    onError: (e) => toast.show('error', errorMessage(e)),
  });

  const canRun = flow.trigger_type === 'manual' && flow.published_version !== null;
  const items: MenuItem[] = [
    { label: 'Edit', icon: Pencil, onSelect: () => navigate(`/flows/${flow.id}`) },
    ...(canRun ? [{ label: 'Run now', icon: Play, onSelect: () => run.mutate(), disabled: run.isPending }] : []),
    { label: 'View history', icon: History, onSelect: () => navigate(`/history?flow_id=${encodeURIComponent(flow.id)}`) },
    {
      label: 'Delete',
      icon: Trash2,
      danger: true,
      onSelect: () => {
        if (window.confirm(`Delete “${flow.name}”? Its history is kept until retention expires.`)) remove.mutate();
      },
    },
  ];
  return { toggle, items };
}
