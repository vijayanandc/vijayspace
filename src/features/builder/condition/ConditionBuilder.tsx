// Visual condition editor: AND/OR rows with one level of sub-groups (JSON fallback otherwise).

import { X } from 'lucide-react';
import { useId, useState } from 'react';

import type { Condition, JsonValue, Op } from '../../../api/types';
import { OPS } from '../../../api/types';
import { JsonInput } from '../../../forms/JsonInput';
import { MappingInput } from '../../../forms/MappingInput';
import {
  type Group, type Mode, NO_RIGHT, OP_LABELS, type Row, type SubGroup, conditionProblem, emptyRow, fromModel, isCondition, isEditable, isSubGroup, toModel,
} from '../model/condition';
import type { VarNode } from '../../../domain/variables';

interface Props {
  value: Condition;
  onChange: (c: Condition) => void;
  variables: VarNode[];
}

function RowEditor({ row, onChange, onRemove, variables, n }: { row: Row; onChange: (r: Row) => void; onRemove: () => void; variables: VarNode[]; n: string }) {
  return (
    <div className="cond-row">
      <MappingInput ariaLabel={`Condition ${n} value`} value={row.left} onChange={(left) => onChange({ ...row, left })} variables={variables} placeholder="{{ trigger.body.amount }}" />
      <select className="input cond-op" aria-label={`Condition ${n} operator`} value={row.op} onChange={(e) => onChange({ ...row, op: e.target.value as Op })}>
        {OPS.map((op) => (
          <option key={op} value={op}>
            {OP_LABELS[op]}
          </option>
        ))}
      </select>
      {!NO_RIGHT.includes(row.op) && (
        <MappingInput ariaLabel={`Condition ${n} compare to`} value={row.right} onChange={(right) => onChange({ ...row, right })} variables={variables} placeholder={row.op === 'in' || row.op === 'not_in' ? 'a, b, c' : 'value'} />
      )}
      <label className="check small" title="Ignore upper/lower case">
        <input type="checkbox" checked={row.case_insensitive} onChange={(e) => onChange({ ...row, case_insensitive: e.target.checked })} />
        Aa
      </label>
      <button type="button" className="icon-btn icon-btn-sm" aria-label={`Remove condition ${n}`} title="Remove condition" onClick={onRemove}>
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  );
}

function ModeToggle({ mode, onChange, label }: { mode: Mode; onChange: (m: Mode) => void; label: string }) {
  return (
    <select className="input cond-mode" aria-label={label} value={mode} onChange={(e) => onChange(e.target.value as Mode)}>
      <option value="all">All of (AND)</option>
      <option value="any">Any of (OR)</option>
    </select>
  );
}

export function ConditionBuilder({ value, onChange, variables }: Props) {
  const [raw, setRaw] = useState(!isEditable(value));
  const jsonId = useId();
  if (raw) {
    return (
      <div>
        <label htmlFor={jsonId} className="field-label">
          Condition (JSON)
        </label>
        <JsonInput
          id={jsonId}
          value={value as unknown as JsonValue}
          validate={(v) => conditionProblem(v)}
          onChange={(v) => {
            if (isCondition(v)) onChange(v);
          }}
          rows={8}
        />
        {isEditable(value) && (
          <button type="button" className="link-btn" onClick={() => setRaw(false)}>
            Back to visual editor
          </button>
        )}
      </div>
    );
  }
  const model = toModel(value);
  const emit = (g: Group) => onChange(fromModel(g));
  const setItem = (i: number, item: Row | SubGroup) => emit({ ...model, items: model.items.map((x, j) => (j === i ? item : x)) });
  const removeItem = (i: number) => emit({ ...model, items: model.items.filter((_, j) => j !== i) });
  return (
    <div className="cond">
      <ModeToggle mode={model.mode} onChange={(mode) => emit({ ...model, mode })} label="Match" />
      {model.items.map((item, i) =>
        isSubGroup(item) ? (
          <div key={i} className="cond-group">
            <div className="cond-group-header">
              <ModeToggle mode={item.mode} onChange={(mode) => setItem(i, { ...item, mode })} label={`Group ${i + 1} match`} />
              <button type="button" className="link-btn" onClick={() => removeItem(i)}>
                Remove group
              </button>
            </div>
            {item.rows.map((row, r) => (
              <RowEditor
                key={r}
                n={`${i + 1}.${r + 1}`}
                row={row}
                variables={variables}
                onChange={(nr) => setItem(i, { ...item, rows: item.rows.map((x, k) => (k === r ? nr : x)) })}
                onRemove={() => setItem(i, { ...item, rows: item.rows.filter((_, k) => k !== r) })}
              />
            ))}
            <button type="button" className="link-btn" onClick={() => setItem(i, { ...item, rows: [...item.rows, emptyRow()] })}>
              + Condition
            </button>
          </div>
        ) : (
          <RowEditor key={i} n={String(i + 1)} row={item} variables={variables} onChange={(nr) => setItem(i, nr)} onRemove={() => removeItem(i)} />
        ),
      )}
      <div className="cond-actions">
        <button type="button" className="link-btn" onClick={() => emit({ ...model, items: [...model.items, emptyRow()] })}>
          + Condition
        </button>
        <button type="button" className="link-btn" onClick={() => emit({ ...model, items: [...model.items, { mode: model.mode === 'all' ? 'any' : 'all', rows: [emptyRow()] }] })}>
          + Group
        </button>
        <button type="button" className="link-btn" onClick={() => setRaw(true)}>
          Edit as JSON
        </button>
      </div>
    </div>
  );
}
