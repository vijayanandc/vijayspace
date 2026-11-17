import type { ScheduleSpec } from '../../../api/types';
import { Field } from '../../../components/Field';
import { type Preset, type PresetState, detectPreset, presetToSpec } from '../model/schedule';

const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const PRESETS: { value: Preset; label: string }[] = [
  { value: 'interval', label: 'Every N minutes' },
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'monthly', label: 'Monthly' },
  { value: 'once', label: 'Once' },
  { value: 'cron', label: 'Cron expression' },
];

function timeZones(): string[] {
  const intl = Intl as unknown as { supportedValuesOf?: (key: string) => string[] };
  return intl.supportedValuesOf?.('timeZone') ?? ['UTC', 'Asia/Kolkata', 'America/New_York', 'Europe/London'];
}

export function SchedulePanel({ spec, onChange }: { spec: ScheduleSpec; onChange: (s: ScheduleSpec) => void }) {
  const state = detectPreset(spec);
  const set = (patch: Partial<PresetState>) => onChange(presetToSpec({ ...state, ...patch }));
  return (
    <div>
      <Field label="Repeat">
        {(id) => (
          <select id={id} className="input" value={state.preset} onChange={(e) => set({ preset: e.target.value as Preset })}>
            {PRESETS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
        )}
      </Field>
      {state.preset === 'interval' && (
        <Field label="Minutes">
          {(id) => <input id={id} className="input" type="number" min={1} value={state.minutes} onChange={(e) => set({ minutes: Number(e.target.value) })} />}
        </Field>
      )}
      {(state.preset === 'daily' || state.preset === 'weekly' || state.preset === 'monthly') && (
        <Field label="At (time)">
          {(id) => <input id={id} className="input" type="time" value={state.time} onChange={(e) => set({ time: e.target.value })} />}
        </Field>
      )}
      {state.preset === 'weekly' && (
        <fieldset className="days">
          <legend className="field-label">On</legend>
          {DAYS.map((d, i) => (
            <label key={d} className="check">
              <input type="checkbox" checked={state.days.includes(i)} onChange={(e) => set({ days: e.target.checked ? [...state.days, i] : state.days.filter((x) => x !== i) })} />
              {d}
            </label>
          ))}
        </fieldset>
      )}
      {state.preset === 'monthly' && (
        <Field label="Day of month">
          {(id) => <input id={id} className="input" type="number" min={1} max={31} value={state.dayOfMonth} onChange={(e) => set({ dayOfMonth: Number(e.target.value) })} />}
        </Field>
      )}
      {state.preset === 'once' && (
        <Field label="Date and time">
          {(id) => <input id={id} className="input" type="datetime-local" value={state.at} onChange={(e) => set({ at: e.target.value })} />}
        </Field>
      )}
      {state.preset === 'cron' && (
        <Field label="Cron expression" help="minute hour day-of-month month day-of-week, e.g. 0 9 * * 1-5">
          {(id) => <input id={id} className="input mono" value={state.expr} onChange={(e) => set({ expr: e.target.value })} />}
        </Field>
      )}
      {state.preset !== 'interval' && (
        <Field label="Time zone">
          {(id) => (
            <>
              <input id={id} className="input" list="weft-timezones" value={state.timezone} onChange={(e) => set({ timezone: e.target.value })} />
              <datalist id="weft-timezones">
                {timeZones().map((tz) => (
                  <option key={tz} value={tz} />
                ))}
              </datalist>
            </>
          )}
        </Field>
      )}
    </div>
  );
}
