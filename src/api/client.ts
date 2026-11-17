// The single fetch wrapper. Every API call goes through `request`; failures become `ApiError`.

import type { Issue } from './types';

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly issues: Issue[];

  constructor(status: number, code: string, message: string, issues: Issue[] = []) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.issues = issues;
  }
}

const BASE: string = (import.meta.env.VITE_API_URL as string | undefined) ?? '';
const ORG: string | undefined = import.meta.env.VITE_ORG_ID as string | undefined;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function asIssues(value: unknown): Issue[] {
  if (!Array.isArray(value)) return [];
  return value.filter(
    (i): i is Issue =>
      isRecord(i) && typeof i.path === 'string' && typeof i.code === 'string' && typeof i.message === 'string',
  );
}

/** Builds an ApiError from a status and a (possibly problem+json) body. */
export function toApiError(status: number, body: unknown, fallbackText = ''): ApiError {
  if (isRecord(body)) {
    const code = typeof body.code === 'string' ? body.code : `http.${status}`;
    const title = typeof body.title === 'string' ? body.title : '';
    const detail = typeof body.detail === 'string' ? body.detail : '';
    const message = [title, detail].filter(Boolean).join(': ') || fallbackText || `Request failed (${status})`;
    return new ApiError(status, code, message, asIssues(body.issues));
  }
  return new ApiError(status, `http.${status}`, fallbackText || `Request failed (${status})`);
}

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

export async function request<T>(method: HttpMethod, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (ORG) headers['X-Org-ID'] = ORG;
  let response: Response;
  try {
    response = await fetch(`${BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (err) {
    throw new ApiError(0, 'network.error', err instanceof Error ? err.message : 'Network error');
  }
  const text = await response.text();
  let parsed: unknown = undefined;
  if (text) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = undefined;
    }
  }
  if (!response.ok) throw toApiError(response.status, parsed, text.slice(0, 300));
  return parsed as T;
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body?: unknown) => request<T>('POST', path, body ?? {}),
  put: <T>(path: string, body: unknown) => request<T>('PUT', path, body),
  patch: <T>(path: string, body: unknown) => request<T>('PATCH', path, body),
  del: (path: string) => request<void>('DELETE', path),
};

/** Human-readable message for any thrown value. */
export function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return err.message;
  if (err instanceof Error) return err.message;
  return String(err);
}
