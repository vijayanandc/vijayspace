// Autosave status chip: Saved / Saving… / Unsaved changes / Save failed (with retry) / Conflict.

import { CircleAlert, CircleCheck, Loader2, PencilLine } from 'lucide-react';

import type { SaveStatus } from './useAutosave';

const CHIP: Record<SaveStatus, { text: string; tone: string }> = {
  saved: { text: 'Saved', tone: 'ok' },
  pending: { text: 'Unsaved changes', tone: 'pending' },
  saving: { text: 'Saving…', tone: 'pending' },
  error: { text: 'Save failed', tone: 'error' },
  conflict: { text: 'Conflict', tone: 'error' },
};

export function SaveChip({ status, error, onRetry }: { status: SaveStatus; error: string | null; onRetry: () => void }) {
  const chip = CHIP[status];
  const Icon = status === 'saved' ? CircleCheck : status === 'saving' ? Loader2 : status === 'pending' ? PencilLine : CircleAlert;
  return (
    <span className={`save-chip save-${chip.tone}`} title={error ?? undefined} role="status">
      <Icon size={14} className={status === 'saving' ? 'spin' : undefined} aria-hidden="true" />
      {chip.text}
      {status === 'error' && (
        <button type="button" className="link-btn" onClick={onRetry}>
          Retry
        </button>
      )}
    </span>
  );
}
