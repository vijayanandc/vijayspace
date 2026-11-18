// The kinds of trigger a flow can start from, with their labels (the new-flow dialog and the trigger panel).

import type { TriggerType } from '../api/types';

export const TRIGGER_CHOICES: { type: TriggerType; label: string; description: string }[] = [
  { type: 'webhook', label: 'Webhook', description: 'An HTTP request to a unique URL starts the flow' },
  { type: 'app', label: 'App event', description: 'A record changes in Zoho CRM, Appblocks, Forma…' },
  { type: 'schedule', label: 'Schedule', description: 'Run once, every N minutes, or on a cron schedule' },
  { type: 'manual', label: 'Manual', description: 'Run on demand with “Run now” or the API' },
];
