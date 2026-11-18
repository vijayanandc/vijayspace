// Today's requests through a connection: "requests / daily limit", a bar coloured by level, and refused requests.

import type { ConnectionUsage } from '../../api/types';
import { usageBarPercent, usageSummary, usageText } from './limits';

const LEVEL_TEXT: Record<ConnectionUsage['level'], string> = {
  ok: '',
  warning: 'Over 80% of the daily budget',
  critical: 'Over 95%: background work paused',
};

/**
 * `detailed` (the Limits dialog) exposes the bar as a meter. The compact form sits inside the row's button, whose
 * children are presentational, so it is hidden there and the button carries the summary as its name.
 */
export function UsageMeter({ usage, detailed = false }: { usage: ConnectionUsage; detailed?: boolean }) {
  const pct = usageBarPercent(usage);
  const max = usage.daily_limit ?? 0;
  const meter = detailed
    ? {
        role: 'meter',
        'aria-label': 'Daily requests used',
        'aria-valuemin': 0,
        'aria-valuemax': max,
        // Inbound events are counted past the budget; a meter's value must stay within its range.
        'aria-valuenow': Math.min(usage.requests, max),
        'aria-valuetext': usageSummary(usage),
      }
    : {};
  return (
    <span className={`usage usage-${usage.level}${detailed ? ' usage-detailed' : ''}`} aria-hidden={detailed ? undefined : true}>
      <span className="usage-line">
        <span className="usage-text">{usageText(usage)}</span>
        {pct !== null && <span className="usage-pct">{Math.floor(usage.percent ?? pct)}%</span>}
      </span>
      {pct !== null && (
        <span className="usage-bar" {...meter}>
          <span className="usage-fill" style={{ width: `${pct}%` }} />
        </span>
      )}
      {usage.refused > 0 && (
        <span className="usage-refused" title="Requests over a limit today. Flow steps retry them later.">
          {usage.refused.toLocaleString('en-US')} refused
        </span>
      )}
      {detailed && LEVEL_TEXT[usage.level] && <span className="usage-level">{LEVEL_TEXT[usage.level]}</span>}
    </span>
  );
}
