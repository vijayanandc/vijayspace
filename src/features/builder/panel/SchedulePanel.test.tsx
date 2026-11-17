import { fireEvent, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import type { ScheduleSpec } from '../../../api/types';
import { renderWithProviders } from '../../../test/render';
import { SchedulePanel } from './SchedulePanel';

function Harness({ initial, onSpec }: { initial: ScheduleSpec; onSpec: (s: ScheduleSpec) => void }) {
  const [spec, setSpec] = useState(initial);
  return (
    <SchedulePanel
      spec={spec}
      onChange={(s) => {
        setSpec(s);
        onSpec(s);
      }}
    />
  );
}

describe('SchedulePanel', () => {
  it('switches a daily schedule to a cron expression and keeps it there while typing', () => {
    const onSpec = vi.fn();
    renderWithProviders(<Harness initial={{ kind: 'cron', expr: '0 9 * * *', timezone: 'UTC' }} onSpec={onSpec} />);
    const repeat = screen.getByLabelText('Repeat');
    expect(repeat).toHaveValue('daily');
    fireEvent.change(repeat, { target: { value: 'cron' } });
    expect(repeat).toHaveValue('cron');
    const expr = screen.getByLabelText(/Cron expression/);
    expect(expr).toHaveValue('0 9 * * *');

    let typed = '';
    for (const ch of '0 9 * * 1-5') {
      typed += ch;
      fireEvent.change(expr, { target: { value: typed } });
      expect(repeat).toHaveValue('cron');
      expect(expr).toHaveValue(typed);
    }
    expect(onSpec).toHaveBeenLastCalledWith({ kind: 'cron', expr: '0 9 * * 1-5', timezone: 'UTC' });
  });

  it('still opens a weekly cron on the Weekly preset', () => {
    renderWithProviders(<Harness initial={{ kind: 'cron', expr: '0 8 * * 1,3', timezone: 'UTC' }} onSpec={vi.fn()} />);
    expect(screen.getByLabelText('Repeat')).toHaveValue('weekly');
  });
});
