import { fireEvent, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { FieldSpec, JsonObject } from '../../../api/types';
import { renderWithProviders } from '../../../test/render';
import { DynamicForm } from './DynamicForm';
import { insertToken, tokenize } from './env';
import { MappingInput } from './MappingInput';

const env = { connector: 'zoho_crm', connectionId: 'cxn_1', requiresConnection: true, variables: [{ label: 'Trigger', path: 'trigger', children: [{ label: 'email', path: 'trigger.email', children: [] }] }] };

function Harness({ fields, initial = {}, onValues }: { fields: FieldSpec[]; initial?: JsonObject; onValues: (v: JsonObject) => void }) {
  const [values, setValues] = useState<JsonObject>(initial);
  return (
    <DynamicForm
      fields={fields}
      values={values}
      env={env}
      onChange={(v) => {
        setValues(v);
        onValues(v);
      }}
    />
  );
}

describe('token helpers', () => {
  it('tokenizes and inserts variables at the cursor', () => {
    expect(tokenize('Hi {{ a }}!')).toEqual([{ text: 'Hi ', token: false }, { text: '{{ a }}', token: true }, { text: '!', token: false }]);
    expect(insertToken('Hello world', 'trigger.name', 6, 11)).toEqual({ value: 'Hello {{ trigger.name }}', cursor: 24 });
  });
});

describe('DynamicForm', () => {
  afterEach(() => vi.restoreAllMocks());

  it('renders fields by type, hides advanced fields and emits values', () => {
    const onValues = vi.fn();
    const fields: FieldSpec[] = [
      { key: 'subject', label: 'Subject', type: 'string', required: true },
      { key: 'method', label: 'Method', type: 'select', options: [{ value: 'GET', label: 'GET' }, { value: 'POST', label: 'POST' }], mappable: false },
      { key: 'html', label: 'HTML', type: 'boolean' },
      { key: 'timeout', label: 'Timeout', type: 'number', advanced: true },
    ];
    renderWithProviders(<Harness fields={fields} onValues={onValues} />);
    fireEvent.change(screen.getByLabelText(/Subject/), { target: { value: 'Hello' } });
    expect(onValues).toHaveBeenLastCalledWith({ subject: 'Hello' });
    fireEvent.change(screen.getByLabelText('Method'), { target: { value: 'POST' } });
    expect(onValues).toHaveBeenLastCalledWith({ subject: 'Hello', method: 'POST' });
    fireEvent.click(screen.getByLabelText('HTML'));
    expect(onValues).toHaveBeenLastCalledWith({ subject: 'Hello', method: 'POST', html: true });
    expect(screen.getByText('Advanced')).toBeInTheDocument();
    expect(screen.getByLabelText('Timeout')).not.toBeVisible();
  });

  it('loads dynamic options with the connection and current input', async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ options: [{ value: 'Leads', label: 'Leads' }, { value: 'Deals', label: 'Deals' }] }), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
    renderWithProviders(<Harness fields={[{ key: 'module', label: 'Module', type: 'select', options_from: 'modules' }]} onValues={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole('option', { name: 'Deals' })).toBeInTheDocument());
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/connectors/zoho_crm/options/modules');
    expect(JSON.parse(String(init.body))).toEqual({ connection_id: 'cxn_1', input: {} });
  });

  it('stores key/value rows as an object', () => {
    const onValues = vi.fn();
    renderWithProviders(<Harness fields={[{ key: 'headers', label: 'Headers', type: 'key_value' }]} onValues={onValues} />);
    fireEvent.click(screen.getByText('+ Add row'));
    fireEvent.change(screen.getByLabelText('Headers key 1'), { target: { value: 'X-Api-Key' } });
    fireEvent.change(screen.getByLabelText('Headers value 1'), { target: { value: 'secret' } });
    expect(onValues).toHaveBeenLastCalledWith({ headers: { 'X-Api-Key': 'secret' } });
  });

  it('keeps key/value rows in place for numeric keys and on duplicate keys', () => {
    const onValues = vi.fn();
    renderWithProviders(<Harness fields={[{ key: 'headers', label: 'Headers', type: 'key_value' }]} initial={{ headers: { name: 'x', other: 'y' } }} onValues={onValues} />);
    fireEvent.change(screen.getByLabelText('Headers key 2'), { target: { value: '1' } });
    fireEvent.change(screen.getByLabelText('Headers key 2'), { target: { value: '10' } });
    expect(screen.getByLabelText('Headers key 1')).toHaveValue('name');
    expect(screen.getByLabelText('Headers key 2')).toHaveValue('10');
    expect(onValues).toHaveBeenLastCalledWith({ headers: { name: 'x', 10: 'y' } });

    fireEvent.change(screen.getByLabelText('Headers key 2'), { target: { value: 'name' } });
    expect(screen.getByLabelText('Headers key 2')).toHaveValue('name');
    expect(screen.getByLabelText('Headers value 2')).toHaveValue('y');
    expect(screen.getByLabelText('Headers key 2')).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByText(/Another row already uses the key “name”/)).toBeInTheDocument();
    expect(onValues).toHaveBeenLastCalledWith({ headers: { name: 'x' } });
  });

  it('lets mappable number fields take decimals and stores the number', () => {
    const onValues = vi.fn();
    renderWithProviders(<Harness fields={[{ key: 'amount', label: 'Amount', type: 'number' }]} onValues={onValues} />);
    const input = screen.getByLabelText('Amount');
    for (const text of ['1', '1.', '1.5', '1.50']) {
      fireEvent.change(input, { target: { value: text } });
      expect(input).toHaveValue(text);
    }
    expect(onValues).toHaveBeenLastCalledWith({ amount: 1.5 });
    fireEvent.change(input, { target: { value: '{{ trigger.email }}' } });
    expect(onValues).toHaveBeenLastCalledWith({ amount: '{{ trigger.email }}' });
  });
});

describe('MappingInput', () => {
  it('inserts a picked variable', () => {
    function M() {
      const [v, setV] = useState('Hi ');
      return <MappingInput ariaLabel="Message" value={v} onChange={setV} variables={env.variables} />;
    }
    renderWithProviders(<M />);
    const input = screen.getByLabelText('Message') as HTMLInputElement;
    input.setSelectionRange(3, 3);
    fireEvent.click(screen.getByLabelText('Insert variable'));
    fireEvent.click(screen.getByRole('button', { name: 'email' }));
    expect(input.value).toBe('Hi {{ trigger.email }}');
  });
});
