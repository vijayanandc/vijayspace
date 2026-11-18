import { fireEvent, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { renderWithProviders } from '../test/render';
import { JSON_PREVIEW_CHARS, JsonView } from './JsonView';
import { LOG_PREVIEW_LINES, LogBlock } from './LogBlock';

describe('large payloads', () => {
  it('shows the first part of a large JSON value until "Show all"', () => {
    const big = { rows: Array.from({ length: 4000 }, (_, i) => ({ id: i, name: `record ${i}`, note: 'x'.repeat(20) })) };
    renderWithProviders(<JsonView value={big} label="Output" />);
    const body = document.querySelector('.json-view-body');
    expect(body?.textContent?.length).toBeLessThan(JSON_PREVIEW_CHARS + 10);
    fireEvent.click(screen.getByRole('button', { name: /Show all/ }));
    expect(document.querySelector('.json-view-body')?.textContent?.length).toBeGreaterThan(JSON_PREVIEW_CHARS);
  });

  it('does not format a collapsed value', () => {
    renderWithProviders(<JsonView value={{ a: 1 }} label="Input" collapsed />);
    expect(document.querySelector('.json-view-body')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /^Input/ }));
    expect(document.querySelector('.json-view-body')?.textContent).toContain('"a": 1');
  });

  it('shows the first lines of a long log until "Show all"', () => {
    const lines = Array.from({ length: LOG_PREVIEW_LINES + 20 }, (_, i) => `line ${i}`);
    renderWithProviders(<LogBlock lines={lines} />);
    expect(screen.getAllByRole('listitem')).toHaveLength(LOG_PREVIEW_LINES);
    fireEvent.click(screen.getByRole('button', { name: `Show all ${lines.length} lines` }));
    expect(screen.getAllByRole('listitem')).toHaveLength(lines.length);
  });
});
