// Custom functions (`core.script`, Rhai): the starter script, examples, and reading logs and errors from results.

import type { ActionStep, JsonObject, JsonValue, Step } from '../api/types';
import { parseLocated } from './code';

export const SCRIPT_CONNECTOR = 'core.script' as const;
export const SCRIPT_ACTION = 'run';
/** Key of the `print(...)` lines in a script's output. */
export const LOGS_KEY = '_logs';

/** An action step of the custom function app (the narrowing keeps other actions on the false branch). */
export type ScriptStep = ActionStep & { connector: typeof SCRIPT_CONNECTOR };

export function isScriptStep(step: Step): step is ScriptStep {
  return step.type === 'action' && step.connector === SCRIPT_CONNECTOR;
}

export const STARTER_SCRIPT = `// Custom function (Rhai). Add input values below and read them as input.<name>.
// The last expression is the step's output: a map becomes its fields.
let name = input.name ?? "world";
print(\`Saying hello to \${name}\`);
#{ message: \`Hello, \${name}!\` }
`;

export interface ScriptExample {
  key: string;
  label: string;
  description: string;
  code: string;
  /** Input values the example reads; inserting it adds the missing ones (empty, ready to map). */
  inputs: string[];
}

export const SCRIPT_EXAMPLES: ScriptExample[] = [
  {
    key: 'sum',
    label: 'Sum line items',
    description: 'Total price × quantity over an array',
    inputs: ['items'],
    code: `// Sum line items. Map an array (e.g. the order's items) to the input value "items".
// Prices and quantities may arrive as numbers or as text ("12.50"): num() reads both.
fn num(v) { if type_of(v) == "string" { parse_float(v) } else { v.to_float() } }
let total = 0.0;
for item in input.items {
    total += num(item.price) * num(item.qty);
}
print(\`\${input.items.len()} item(s)\`);
#{ total: total, is_big: total > 100.0 }
`,
  },
  {
    key: 'date',
    label: 'Format a date',
    description: 'Readable date in a time zone, and a reminder a day earlier',
    inputs: ['due'],
    code: `// Format a date. Map a timestamp to the input value "due" (empty = the step time).
let due = if input.due == () || input.due == "" { context.now } else { input.due };
#{
    due_label: format_date(due, "%d %b %Y, %H:%M", "Asia/Kolkata"),
    remind_at: date_add(due, -1, "days"),
}
`,
  },
  {
    key: 'validate',
    label: 'Validate and throw',
    description: 'Fail the step with a clear message',
    inputs: ['email'],
    code: `// Validate input. \`throw\` fails the step with the message (code script.error).
let email = input.email ?? "";
if !regex_match(email, "^[^@\\\\s]+@[^@\\\\s]+$") {
    throw \`Not a valid email address: \${email}\`;
}
#{ email: email.to_lower() }
`,
  },
];

/** Step input for a newly added custom function. */
export function starterScriptInput(): JsonObject {
  return { code: STARTER_SCRIPT, input: {} };
}

/** Puts an example's code into a step input and adds the input values it reads (keeping existing ones). */
export function withExample(input: JsonObject, example: ScriptExample): JsonObject {
  const current = input.input;
  const values: JsonObject = current && typeof current === 'object' && !Array.isArray(current) ? { ...current } : {};
  for (const name of example.inputs) if (!(name in values)) values[name] = '';
  return { ...input, code: example.code, input: values };
}

/** True when replacing `code` would lose the user's own work (not empty, the starter or an unchanged example). */
export function hasOwnCode(code: JsonValue | undefined): boolean {
  if (typeof code !== 'string' || code.trim() === '') return false;
  return code !== STARTER_SCRIPT && !SCRIPT_EXAMPLES.some((e) => e.code === code);
}

/**
 * True for a step run of a custom function. Step runs do not record their app, so this reads the rendered input,
 * which for `core.script` is `{code: "…", input: {…}}`.
 */
export function isScriptRun(run: { kind: string; input: JsonValue }): boolean {
  const input = run.input;
  if (run.kind !== 'action' || !input || typeof input !== 'object' || Array.isArray(input)) return false;
  const values = input.input;
  // `input` is a key/value field: an object, or absent / null when no values were added.
  return typeof input.code === 'string' && (values === undefined || values === null || (typeof values === 'object' && !Array.isArray(values)));
}

/** Splits a script's output into its `print` lines and the rest. */
export function splitLogs(output: JsonValue | undefined): { logs: string[]; rest: JsonValue | undefined } {
  if (!output || typeof output !== 'object' || Array.isArray(output) || !(LOGS_KEY in output)) return { logs: [], rest: output };
  const { [LOGS_KEY]: raw, ...rest } = output;
  const logs = Array.isArray(raw) ? raw.map((l) => (typeof l === 'string' ? l : JSON.stringify(l))) : [];
  return { logs, rest };
}

export interface ScriptErrorInfo {
  code: string;
  line?: number;
  column?: number;
  /** The message without its "line N, column M:" prefix. */
  text: string;
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

const asPosition = (v: unknown): number | undefined => (typeof v === 'number' && Number.isInteger(v) && v > 0 ? v : undefined);

/**
 * Reads a `script.*` error (a step run's `error`, or a step test's) into its line, column and text.
 * Step runs carry `details: {line, column}`; test results only have them in the message. Other errors → null.
 */
export function scriptError(error: unknown): ScriptErrorInfo | null {
  if (!isRecord(error) || typeof error.code !== 'string' || !error.code.startsWith('script.')) return null;
  const message = typeof error.message === 'string' ? error.message : '';
  const parsed = parseLocated(message);
  const details = isRecord(error.details) ? error.details : {};
  return {
    code: error.code,
    line: asPosition(details.line) ?? parsed.line,
    column: asPosition(details.column) ?? parsed.column,
    text: parsed.text,
  };
}
