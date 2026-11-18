// Saves a connection, then finds out why it does not work when its test failed, so the dialog can stay open with
// the app's own reason instead of closing on a broken connection.

import { endpoints } from '../../api/endpoints';
import type { Connection } from '../../api/types';

export interface SaveOutcome {
  connection: Connection;
  /** Why the connection test failed (the app's answer); absent when it works. */
  problem?: string;
}

/** `test`: also test a connection reported active (after an edit, which does not test by itself). */
export async function saveAndTest(save: () => Promise<Connection>, test = false): Promise<SaveOutcome> {
  const connection = await save();
  if (connection.status === 'active' && !test) return { connection };
  const res = await endpoints.testConnection(connection.id);
  if (res.ok) return { connection: { ...connection, status: 'active' } };
  return { connection: { ...connection, status: 'error' }, problem: res.error.message };
}
