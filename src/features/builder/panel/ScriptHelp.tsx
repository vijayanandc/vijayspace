// Custom function help (collapsible): how input, context, output, throw and print work, the helper list,
// and an "Insert example" menu.

import { BookOpen, ChevronDown, ChevronRight, Sparkles } from 'lucide-react';
import { useId, useState } from 'react';

import { Popover } from '../../../components/Popover';
import { SCRIPT_EXAMPLES, type ScriptExample } from '../../../domain/script';

const HELPERS: [string, string][] = [
  ['parse_json(text)', 'JSON text → map or array'],
  ['to_json(value)', 'Any value → JSON text'],
  ['format_date(value, fmt[, tz])', 'Format a date, e.g. "%d %b %Y" in "Asia/Kolkata"'],
  ['date_add(value, amount, unit)', 'Add seconds, minutes, hours, days or weeks'],
  ['regex_replace(text, pattern, with)', 'Replace every match'],
  ['regex_match(text, pattern)', 'true when the pattern matches'],
  ['base64_encode(text) / base64_decode(text)', 'Base64'],
  ['url_encode(text) / url_decode(text)', 'Percent-encoding'],
  ['sha256(text)', 'Hex SHA-256 digest'],
  ['hmac_sha256(text, key)', 'Hex HMAC-SHA256 signature'],
];

function ExampleMenu({ onInsert }: { onInsert: (example: ScriptExample) => void }) {
  return (
    <Popover
      className="example-menu"
      trigger={(open, toggle) => (
        <button type="button" className="btn btn-sm btn-secondary" aria-haspopup="menu" aria-expanded={open} onClick={toggle}>
          <Sparkles size={14} aria-hidden="true" />
          Insert example
          <ChevronDown size={14} aria-hidden="true" />
        </button>
      )}
    >
      {(close) => (
        <div role="menu" className="menu" aria-label="Examples">
          {SCRIPT_EXAMPLES.map((ex) => (
            <button
              key={ex.key}
              type="button"
              role="menuitem"
              className="menu-item example-item"
              onClick={() => {
                close();
                onInsert(ex);
              }}
            >
              <span className="example-label">{ex.label}</span>
              <span className="example-desc">{ex.description}</span>
            </button>
          ))}
        </div>
      )}
    </Popover>
  );
}

export function ScriptHelp({ stepKey, onInsert }: { stepKey: string; onInsert: (example: ScriptExample) => void }) {
  const [open, setOpen] = useState(false);
  const bodyId = useId();
  return (
    <div className="script-help">
      <div className="script-help-bar">
        <button type="button" className="link-btn" aria-expanded={open} aria-controls={bodyId} onClick={() => setOpen((v) => !v)}>
          {open ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronRight size={14} aria-hidden="true" />}
          <BookOpen size={14} aria-hidden="true" />
          How custom functions work
        </button>
        <ExampleMenu onInsert={onInsert} />
      </div>
      {open && (
        <div id={bodyId} className="script-help-body">
          <dl className="script-rules">
            <dt>Input</dt>
            <dd>
              Add <em>Input values</em> below and map data from earlier steps into them. The script reads them as <code>input.&lt;name&gt;</code>, e.g. <code>input.items</code>.
              Changing <code>input</code> only changes this step’s copy.
            </dd>
            <dt>Functions</dt>
            <dd>
              A <code>fn</code> sees only its parameters, so pass values in: <code>fn total(items) {'{'} … {'}'}</code>, then <code>total(input.items)</code>. Closures such as{' '}
              <code>|x| x * 2</code> can read variables.
            </dd>
            <dt>Time</dt>
            <dd>
              <code>context.now</code> is the step’s time (UTC). Scripts have no clock, network or files, so the same input always gives the same output.
            </dd>
            <dt>Output</dt>
            <dd>
              The last expression is the step’s output. A map such as <code>#{'{'} total: 10 {'}'}</code> becomes its fields; any other value becomes <code>result</code>. Later steps use{' '}
              <code>{`{{ steps.${stepKey}.total }}`}</code>.
            </dd>
            <dt>Errors</dt>
            <dd>
              <code>throw "message"</code> fails the step with <code>script.error</code>. Script errors are not retried.
            </dd>
            <dt>Logging</dt>
            <dd>
              <code>print(…)</code> lines are kept in the output as <code>_logs</code> and shown as a log in tests and history.
            </dd>
            <dt>Templates</dt>
            <dd>
              Code cannot contain <code>{'{{'}</code> or <code>{'{%'}</code> (they start templates). Map values through the inputs instead.
            </dd>
          </dl>
          <h4 className="script-help-title">Helpers</h4>
          <dl className="helper-list">
            {HELPERS.map(([sig, what]) => (
              <div key={sig}>
                <dt>
                  <code>{sig}</code>
                </dt>
                <dd>{what}</dd>
              </div>
            ))}
          </dl>
        </div>
      )}
    </div>
  );
}
