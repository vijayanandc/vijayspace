// The run log's filters, offsets, detail rows and plain-text copy.

import { describe, expect, it } from 'vitest';

import type { LogEntry } from '../../../api/types';
import { detailRows, filterCounts, filterLog, formatBytes, logAsText, offsetLabel, stepKeysOf } from './runLog';

const line = (over: Partial<LogEntry>): LogEntry => ({
  at: '2026-11-18T09:00:00.000Z',
  level: 'info',
  kind: 'step',
  step_key: null,
  attempt: null,
  message: 'm',
  details: null,
  ...over,
});

const http = line({
  at: '2026-11-18T09:00:01.500Z',
  level: 'warn',
  kind: 'http',
  step_key: 'crm',
  attempt: 1,
  message: 'POST https://crm.example.com/v2/Leads → 503 in 120 ms',
  // As MySQL returns JSON objects: keys re-ordered.
  details: { url: 'https://crm.example.com/v2/Leads', error: null, method: 'POST', status: 503, duration_ms: 120, request_bytes: 2048, response_bytes: 15, response_excerpt: 'busy, try later' },
});
const log: LogEntry[] = [line({ kind: 'run', message: 'Run queued (live)' }), line({ step_key: 'crm', attempt: 1, message: 'Step `crm` started (attempt 1)' }), http, line({ level: 'error', step_key: 'mail', kind: 'step', message: 'Step `mail` failed' })];

describe('run log', () => {
  it('filters by problems, HTTP calls and step', () => {
    expect(filterLog(log, 'problems', null).map((e) => e.message)).toEqual([http.message, 'Step `mail` failed']);
    expect(filterLog(log, 'http', null)).toEqual([http]);
    expect(filterLog(log, 'all', 'crm')).toHaveLength(2);
    expect(filterCounts(log)).toEqual({ all: 4, problems: 2, http: 1 });
    expect(stepKeysOf(log)).toEqual(['crm', 'mail']);
  });

  it('shows offsets from the run start and readable sizes', () => {
    expect(offsetLabel('2026-11-18T09:00:00.182Z', '2026-11-18T09:00:00.000Z')).toBe('+182 ms');
    expect(offsetLabel('2026-11-18T09:00:02.400Z', '2026-11-18T09:00:00.000Z')).toBe('+2.4 s');
    expect(offsetLabel('2026-11-18T09:03:05.000Z', '2026-11-18T09:00:00.000Z')).toBe('+3 min 5 s');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
  });

  it('lists details in reading order without empty values or the answer, and copies as text', () => {
    expect(detailRows(http).map((r) => `${r.label}: ${r.value}`)).toEqual([
      'Method: POST',
      'URL: https://crm.example.com/v2/Leads',
      'Status: 503',
      'Time: 120 ms',
      'Sent: 2.0 KB',
      'Received: 15 B',
    ]);
    expect(logAsText([http])).toBe('2026-11-18T09:00:01.500Z WARN  [crm #1] POST https://crm.example.com/v2/Leads → 503 in 120 ms\n    busy, try later');
  });
});
