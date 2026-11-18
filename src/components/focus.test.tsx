import { fireEvent, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../test/render';
import { Dialog } from './Dialog';
import { Field } from './Field';
import { OverflowMenu } from './Popover';

function MenuWithDialog() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <OverflowMenu label="More actions for Orders" items={[{ label: 'Rename', onSelect: () => undefined }, { label: 'Limits', onSelect: () => setOpen(true) }]} />
      <Dialog title="Limits" open={open} onClose={() => setOpen(false)} footer={<button type="button">Save</button>}>
        <input aria-label="Per minute" />
      </Dialog>
    </>
  );
}

describe('keyboard focus', () => {
  it('moves through a menu with the arrows, and Escape returns to its button', () => {
    renderWithProviders(<MenuWithDialog />);
    const trigger = screen.getByRole('button', { name: 'More actions for Orders' });
    fireEvent.click(trigger);
    expect(screen.getByRole('menuitem', { name: 'Rename' })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'ArrowDown' });
    expect(screen.getByRole('menuitem', { name: 'Limits' })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole('menu'), { key: 'ArrowDown' });
    expect(screen.getByRole('menuitem', { name: 'Rename' })).toHaveFocus();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it('keeps Tab inside a dialog opened from a menu, and gives focus back to the menu button', () => {
    renderWithProviders(<MenuWithDialog />);
    const trigger = screen.getByRole('button', { name: 'More actions for Orders' });
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Limits' }));
    const save = screen.getByRole('button', { name: 'Save' });
    save.focus();
    fireEvent.keyDown(document, { key: 'Tab' });
    expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus();
    fireEvent.keyDown(document, { key: 'Tab', shiftKey: true });
    expect(save).toHaveFocus();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});

describe('form fields', () => {
  it('link the control to its help and error, and mark it required and invalid', () => {
    renderWithProviders(
      <Field label="Email address" required help="We send the invitation here." error="Enter an email address.">
        {(id) => <input id={id} />}
      </Field>,
    );
    const input = screen.getByRole('textbox', { name: /Email address/ });
    expect(input).toHaveAttribute('aria-required', 'true');
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAccessibleDescription('We send the invitation here. Enter an email address.');
  });
});
