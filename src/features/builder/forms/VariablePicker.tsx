// Variable picker popover: a searchable tree of trigger, step and variable outputs with sample values.

import { ChevronDown, ChevronRight, X } from 'lucide-react';
import { useMemo, useState } from 'react';

import type { VarNode } from '../model/variables';

function matches(node: VarNode, q: string): boolean {
  return node.label.toLowerCase().includes(q) || node.path.toLowerCase().includes(q) || node.children.some((c) => matches(c, q));
}

function preview(sample: unknown): string {
  if (sample === undefined) return '';
  const text = typeof sample === 'string' ? sample : JSON.stringify(sample);
  return text.length > 40 ? `${text.slice(0, 40)}…` : text;
}

function NodeRow({ node, depth, q, onPick }: { node: VarNode; depth: number; q: string; onPick: (path: string) => void }) {
  const [open, setOpen] = useState(depth === 0);
  const expanded = open || q !== '';
  const hasChildren = node.children.length > 0;
  return (
    <li>
      <div className="var-row" style={{ paddingLeft: depth * 12 }}>
        {hasChildren ? (
          <button type="button" className="var-toggle" aria-label={expanded ? 'Collapse' : 'Expand'} onClick={() => setOpen(!expanded)}>
            {expanded ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}
          </button>
        ) : (
          <span className="var-toggle" />
        )}
        {node.path ? (
          <button type="button" className="var-pick" onClick={() => onPick(node.path)} title={`{{ ${node.path} }}`}>
            {node.label}
            {node.maybe && <span className="muted small"> (may not run)</span>}
          </button>
        ) : (
          <span className="var-group">{node.label}</span>
        )}
        <span className="var-sample">{preview(node.sample)}</span>
      </div>
      {hasChildren && expanded && (
        <ul>
          {node.children
            .filter((c) => !q || matches(c, q))
            .map((c) => (
              <NodeRow key={`${c.path}:${c.label}`} node={c} depth={depth + 1} q={q} onPick={onPick} />
            ))}
        </ul>
      )}
    </li>
  );
}

export function VariablePicker({ tree, onPick, onClose }: { tree: VarNode[]; onPick: (path: string) => void; onClose: () => void }) {
  const [query, setQuery] = useState('');
  const q = query.trim().toLowerCase();
  const visible = useMemo(() => tree.filter((n) => !q || matches(n, q)), [tree, q]);
  return (
    <div className="var-picker" role="dialog" aria-label="Insert variable" onKeyDown={(e) => e.key === 'Escape' && onClose()}>
      <div className="var-picker-header">
        <input className="input" autoFocus placeholder="Search variables" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search variables" />
        <button type="button" className="icon-btn icon-btn-sm" aria-label="Close" onClick={onClose}>
          <X size={14} aria-hidden="true" />
        </button>
      </div>
      <ul className="var-tree">
        {visible.map((n) => (
          <NodeRow key={n.label} node={n} depth={0} q={q} onPick={onPick} />
        ))}
      </ul>
    </div>
  );
}
