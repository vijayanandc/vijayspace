import { fireEvent, screen } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../test/render';
import { Dialog } from './Dialog';
import { ErrorBoundary } from './ErrorBoundary';
import { JsonView } from './JsonView';

describe('Dialog', () => {
  it('closes only the topmost of two nested dialogs on Escape', () => {
    function Nested() {
      const [outer, setOuter] = useState(true);
      const [inner, setInner] = useState(false);
      return (
        <Dialog title="Install" open={outer} onClose={() => setOuter(false)}>
          <button type="button" onClick={() => setInner(true)}>
            New connection
          </button>
          <Dialog title="Connect" open={inner} onClose={() => setInner(false)}>
            <input aria-label="Name" />
          </Dialog>
        </Dialog>
      );
    }
    renderWithProviders(<Nested />);
    fireEvent.click(screen.getByRole('button', { name: 'New connection' }));
    expect(screen.getAllByRole('dialog')).toHaveLength(2);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.getByRole('dialog', { name: 'Install' })).toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: 'Connect' })).not.toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});

describe('ErrorBoundary', () => {
  afterEach(() => vi.restoreAllMocks());

  it('shows a recoverable message instead of a blank page when rendering fails', () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    let broken = true;
    function Fragile() {
      if (broken) throw new Error('Cannot read properties of undefined');
      return <p>Recovered</p>;
    }
    renderWithProviders(
      <ErrorBoundary>
        <Fragile />
      </ErrorBoundary>,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Something went wrong on this page.');
    expect(screen.getByRole('alert')).toHaveTextContent('Cannot read properties of undefined');
    broken = false;
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(screen.getByText('Recovered')).toBeInTheDocument();
  });
});

describe('copying', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('reports a refused clipboard write instead of claiming it copied', async () => {
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText: () => Promise.reject(new DOMException('denied', 'NotAllowedError')) } });
    renderWithProviders(<JsonView label="Output" value={{ a: 1 }} />);
    fireEvent.click(screen.getByRole('button', { name: 'Copy Output' }));
    expect(await screen.findByText(/Could not copy to the clipboard/)).toBeInTheDocument();
    expect(screen.queryByText('Copied')).not.toBeInTheDocument();
  });

  it('shows Copied when the write succeeds', async () => {
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });
    renderWithProviders(<JsonView label="Output" value={{ a: 1 }} />);
    fireEvent.click(screen.getByRole('button', { name: 'Copy Output' }));
    expect(await screen.findByText('Copied')).toBeInTheDocument();
    expect(writeText).toHaveBeenCalledWith('{\n  "a": 1\n}');
  });
});
