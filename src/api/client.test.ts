import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiError, request, toApiError } from './client';

describe('api client', () => {
  afterEach(() => vi.restoreAllMocks());

  it('parses problem+json into ApiError with issues', () => {
    const err = toApiError(422, {
      title: 'Flow is not valid', status: 422, code: 'flow.invalid', detail: 'Fix 1 issue.',
      issues: [{ severity: 'error', path: '/steps/0', code: 'field.required', message: 'x' }, { bogus: true }],
    });
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe('flow.invalid');
    expect(err.message).toBe('Flow is not valid: Fix 1 issue.');
    expect(err.issues).toHaveLength(1);
  });

  it('falls back for non-JSON errors', () => {
    const err = toApiError(502, undefined, 'Bad gateway');
    expect(err.code).toBe('http.502');
    expect(err.message).toBe('Bad gateway');
  });

  it('sends JSON and returns parsed bodies; 204 is undefined', async () => {
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
      if (init?.method === 'DELETE') return new Response(null, { status: 204 });
      return new Response(JSON.stringify({ ok: true, echo: init?.body }), { status: 200 });
    });
    vi.stubGlobal('fetch', fetchMock);
    await expect(request('POST', '/api/v1/x', { a: 1 })).resolves.toEqual({ ok: true, echo: '{"a":1}' });
    await expect(request('DELETE', '/api/v1/x')).resolves.toBeUndefined();
  });

  it('throws ApiError for error statuses and network failures', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ code: 'flow.not_found', title: 'Not found' }), { status: 404 })));
    await expect(request('GET', '/api/v1/flows/x')).rejects.toMatchObject({ status: 404, code: 'flow.not_found' });
    vi.stubGlobal('fetch', vi.fn(async () => { throw new TypeError('offline'); }));
    await expect(request('GET', '/x')).rejects.toMatchObject({ status: 0, code: 'network.error' });
  });
});
