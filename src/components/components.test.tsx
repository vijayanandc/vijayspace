import { fireEvent, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import type { SortState } from '../lib/sort';
import { renderWithProviders } from '../test/render';
import { AppIcon } from './AppIcon';
import { OverflowMenu } from './Popover';
import { Switch } from './Switch';
import { SortHeader } from './Table';

describe('Switch', () => {
  it('is a labelled checkbox that toggles when its label is clicked', () => {
    function Harness() {
      const [on, setOn] = useState(false);
      return <Switch checked={on} onChange={() => setOn(!on)} label="Flow enabled" />;
    }
    renderWithProviders(<Harness />);
    const box = screen.getByRole('checkbox', { name: 'Flow enabled' });
    expect(screen.getByText('Off')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Off'));
    expect(box).toBeChecked();
    expect(screen.getByText('On')).toBeInTheDocument();
  });
});

describe('OverflowMenu', () => {
  it('opens, runs the chosen action and closes; Escape closes it too', () => {
    const onEdit = vi.fn();
    renderWithProviders(<OverflowMenu label="More actions for Demo" items={[{ label: 'Edit', onSelect: onEdit }, { label: 'Delete', onSelect: vi.fn(), danger: true }]} />);
    const trigger = screen.getByRole('button', { name: 'More actions for Demo' });
    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    fireEvent.click(screen.getByRole('menuitem', { name: 'Edit' }));
    expect(onEdit).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    fireEvent.click(trigger);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });
});

describe('AppIcon', () => {
  it('uses monogram tiles for known apps and the first letter for unknown ones', () => {
    renderWithProviders(
      <>
        <AppIcon app="zoho_crm" name="Zoho CRM" />
        <AppIcon app="acme.billing" name="Billing" />
      </>,
    );
    expect(screen.getByRole('img', { name: 'Zoho CRM' })).toHaveTextContent('CRM');
    expect(screen.getByRole('img', { name: 'Billing' })).toHaveTextContent('B');
  });
});

describe('SortHeader', () => {
  it('reports the sort direction and asks to sort by its column', () => {
    const onSort = vi.fn();
    const sort: SortState<'name' | 'date'> = { key: 'name', dir: 'desc' };
    renderWithProviders(
      <table>
        <thead>
          <tr>
            <SortHeader label="Name" column="name" sort={sort} onSort={onSort} />
            <SortHeader label="Date" column="date" sort={sort} onSort={onSort} />
          </tr>
        </thead>
      </table>,
    );
    expect(screen.getByRole('columnheader', { name: 'Name' })).toHaveAttribute('aria-sort', 'descending');
    expect(screen.getByRole('columnheader', { name: 'Date' })).toHaveAttribute('aria-sort', 'none');
    fireEvent.click(screen.getByRole('button', { name: 'Date' }));
    expect(onSort).toHaveBeenCalledWith('date');
  });
});
