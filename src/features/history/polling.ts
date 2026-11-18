// How often run history refreshes: fast while a run moves, slowly while it waits, never once it is finished or
// cannot be loaded; the list only while a single page is shown.

import { isTerminal } from '../../lib/format';

const RUNNING_MS = 2000;
const WAITING_MS = 30_000;
const MAX_WAIT_MS = 60_000;

/** The refetch interval of an open execution (`false`: stop). */
export function executionPollMs(status: string | undefined, failed: boolean, waitingUntil: string | null, now: number): number | false {
  if (failed) return false;
  if (status === undefined) return RUNNING_MS;
  if (isTerminal(status)) return false;
  if (status !== 'waiting') return RUNNING_MS;
  const due = waitingUntil ? Date.parse(waitingUntil) - now : Number.NaN;
  // A delay that ends soon is looked at when it ends; a long one every half minute (it may be cancelled).
  return Number.isFinite(due) && due > 0 ? Math.min(Math.max(due, RUNNING_MS), MAX_WAIT_MS) : WAITING_MS;
}

/** The list refreshes only its first page: with more pages loaded, refreshing would download every one again. */
export function historyPollMs(pagesLoaded: number): number | false {
  return pagesLoaded <= 1 ? 5000 : false;
}
