import { fireEvent, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import type { Condition } from '../../../api/types';
import { renderWithProviders } from '../../../test/render';
import { ConditionBuilder } from './ConditionBuilder';

function Harness({ initial, onCondition }: { initial: Condition; onCondition: (c: Condition) => void }) {
  const [c, setC] = useState(initial);
  return (
    <ConditionBuilder
      value={c}
      variables={[]}
      onChange={(next) => {
        setC(next);
        onCondition(next);
      }}
    />
  );
}

describe('ConditionBuilder', () => {
  it('edits rows, operators and groups into the contract shape', () => {
    const onCondition = vi.fn();
    renderWithProviders(<Harness initial={{ all: [{ left: '', op: 'eq', right: '' }] }} onCondition={onCondition} />);
    fireEvent.change(screen.getByLabelText('Condition 1 value'), { target: { value: '{{ trigger.amount }}' } });
    fireEvent.change(screen.getByLabelText('Condition 1 operator'), { target: { value: 'gt' } });
    fireEvent.change(screen.getByLabelText('Condition 1 compare to'), { target: { value: '100' } });
    expect(onCondition).toHaveBeenLastCalledWith({ all: [{ left: '{{ trigger.amount }}', op: 'gt', right: '100' }] });

    fireEvent.change(screen.getByLabelText('Match'), { target: { value: 'any' } });
    fireEvent.click(screen.getByText('+ Group'));
    fireEvent.change(screen.getByLabelText('Condition 2.1 operator'), { target: { value: 'is_true' } });
    expect(onCondition).toHaveBeenLastCalledWith({
      any: [{ left: '{{ trigger.amount }}', op: 'gt', right: '100' }, { all: [{ left: '', op: 'is_true' }] }],
    });
    expect(screen.queryByLabelText('Condition 2.1 compare to')).not.toBeInTheDocument();
  });

  it('falls back to JSON for conditions the visual editor cannot show', () => {
    renderWithProviders(<Harness initial={{ not: { left: 'a', op: 'eq', right: 'b' } }} onCondition={vi.fn()} />);
    expect(screen.getByDisplayValue(/"not"/)).toBeInTheDocument();
  });
});
