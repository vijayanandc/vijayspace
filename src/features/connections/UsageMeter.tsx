// Today's requests through a connection: "requests / daily limit", a bar coloured by level, and refused requests.

import type { ConnectionUsage } from '../../api/types';
import { usageBarPercent, usageText } from './limits';

const LEVEL_TEXT: Record<ConnectionUsage['level'], string> = {
  ok: '',
  warning: 'Over 80% of the daily budget',
  critical: 'Over 95%: background work paused',
};

export function UsageMeter({ usage, detailed = false }: { usage: ConnectionUsage; detailed?: boolean }) {
  const pct = usageBarPercent(usage);
  return (
    <span className={`usage usage-${usage.level}${detailed ? ' usage-detailed' : ''}`}>
      <span className="usage-line">
        <span className="usage-text">{usageText(usage)}</span>
        {pct !== null && <span className="usage-pct">{Math.floor(usage.percent ?? pct)}%</span>}
      </span>
      {pct !== null && (
        <span
          className="usage-bar"
          role="meter"
          aria-label="Daily requests used"
          aria-valuemin={0}
          aria-valuemax={usage.daily_limit ?? 0}
          aria-valuenow={usage.requests}
          aria-valuetext={`${usageText(usage)} requests today`}
        >
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
