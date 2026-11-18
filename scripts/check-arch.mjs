// Architecture check (`npm run arch`, part of `npm run check`): layer boundaries, feature public APIs, no feature
// cycles, restricted browser APIs, file headers and sizes. The rules and their reasons are in AGENTS.md
// ("Architecture"). Run `node scripts/check-arch.mjs` to see every violation with how to fix it.

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), '..');
const SRC = join(ROOT, 'src');
const MAX_LINES = 300;

/** Which layers each layer may import from (runtime and type imports alike). */
const ALLOWED = {
  app: ['app', 'feature-index', 'api', 'components', 'lib', 'styles'],
  api: ['api', 'contract', 'lib'],
  contract: ['contract'],
  lib: ['lib'],
  domain: ['domain', 'lib', 'contract'],
  components: ['components', 'lib', 'contract'],
  forms: ['forms', 'components', 'domain', 'api', 'lib'],
  model: ['own-model', 'domain', 'lib', 'contract'],
  feature: ['own-feature', 'feature-index', 'api', 'components', 'forms', 'domain', 'lib'],
  test: ['*'],
};
/** Layers that must stay pure: no runtime imports of packages (React, the router, the query client…). */
const PURE = new Set(['domain', 'model']);

const FIX = {
  layer: 'move the shared code down a layer (pure flow logic → src/domain, FieldSpec inputs → src/forms, helpers → src/lib), or pass it in as a prop',
  index: 'import the other feature through its index.ts (export what you need there)',
  pure: 'keep this file pure: take data as arguments; React, queries and HTTP belong in components and hooks',
  cycle: 'features must form a tree: move the shared part to src/domain, src/forms or src/components, or pass it as a prop',
};

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

/** The layer of a file under src/, with its feature name when it belongs to one. */
function layerOf(file) {
  const rel = relative(SRC, file).split('/');
  if (rel.length === 1) return { layer: 'app' };
  if (rel[0] === 'features') {
    const feature = rel[1];
    if (rel.length === 3 && rel[2] === 'index.ts') return { layer: 'feature-index', feature };
    return { layer: rel[2] === 'model' ? 'model' : 'feature', feature };
  }
  // The contract types (mirroring the backend's docs/api/README.md): pure, importable from every layer.
  if (rel[0] === 'api' && (rel[1] === 'types.ts' || rel[1] === 'definition.ts')) return { layer: 'contract' };
  return { layer: rel[0] };
}

const isTest = (file) => /\.test\.tsx?$/.test(file) || relative(SRC, file).startsWith('test/');

function resolveImport(from, spec) {
  if (!spec.startsWith('.')) return null;
  const base = resolve(dirname(from), spec);
  for (const ext of ['', '.ts', '.tsx', '/index.ts', '/index.tsx']) {
    if (existsSync(base + ext) && statSync(base + ext).isFile()) return base + ext;
  }
  return base;
}

/** Static `import`/`export … from` and dynamic `import()` specifiers, with line numbers. */
function importsOf(text) {
  const out = [];
  const lineAt = (i) => text.slice(0, i).split('\n').length;
  const stat = /^\s*(?:import|export)\s+(type\s+)?(?:[\w*{}\s,$]+?\s+from\s+)?['"]([^'"]+)['"]/gm;
  for (const m of text.matchAll(stat)) out.push({ spec: m[2], typeOnly: Boolean(m[1]), line: lineAt(m.index) });
  for (const m of text.matchAll(/\bimport\(\s*['"]([^'"]+)['"]\s*\)/g)) out.push({ spec: m[1], typeOnly: false, line: lineAt(m.index) });
  return out;
}

function allowed(from, to) {
  const rules = ALLOWED[isTest(from.file) ? 'test' : from.layer] ?? [];
  if (rules.includes('*')) return true;
  if (to.layer === 'contract' && (rules.includes('api') || rules.includes('contract'))) return true;
  if (to.layer === 'feature-index' && to.feature === from.feature) return rules.includes('own-feature');
  if (to.feature && to.feature === from.feature) {
    return rules.includes('own-feature') || (rules.includes('own-model') && to.layer === 'model');
  }
  return rules.includes(to.layer);
}

const RESTRICTED = [
  { re: /(?<![\w.$])fetch\s*\(/, only: 'src/api/client.ts', why: 'all HTTP goes through src/api/client.ts (sign-in, org header, errors)' },
  { re: /\b(?:localStorage|sessionStorage)\b/, only: 'src/lib/storage.ts', why: 'browser storage goes through src/lib/storage.ts (it can be unavailable)' },
  { re: /dangerouslySetInnerHTML|\.innerHTML\b|outerHTML|insertAdjacentHTML|document\.write/, why: 'no HTML injection: render text through React' },
  { re: /(?<![\w.])eval\s*\(|new Function\s*\(/, why: 'no code evaluation in the browser' },
  { re: /@ts-ignore|@ts-nocheck/, why: 'fix the type, or use @ts-expect-error with a reason' },
  { re: /eslint-disable(?:-next-line|-line)?\s*(?:\*\/)?\s*$/, why: 'name the rule you disable, and say why' },
  { re: /^export default\b/, why: 'use named exports (one name per thing, easy to find and rename)' },
  { re: /window\.open\((?![^)]*noopener)/, why: 'window.open needs "noopener,noreferrer"' },
  { re: /target=["']_blank["'](?![^>]*rel=["'][^"']*noopener)/, why: 'target="_blank" needs rel="noopener noreferrer"' },
];

const problems = [];
const report = (file, line, rule, message) => problems.push(`${relative(ROOT, file)}:${line}  [${rule}] ${message}`);
const files = walk(SRC).filter((f) => /\.(tsx?|css)$/.test(f));
const featureEdges = new Map();

for (const file of files) {
  const text = readFileSync(file, 'utf8');
  const lines = text.split('\n');
  const count = text.endsWith('\n') ? lines.length - 1 : lines.length;
  if (count > MAX_LINES) report(file, MAX_LINES + 1, 'size', `${count} lines (max ${MAX_LINES}): split it by responsibility`);
  if (file.endsWith('.css')) continue;
  const test = isTest(file);
  if (!test && !/^(\/\/|\/\*)/.test(lines[0] ?? '')) report(file, 1, 'header', 'start with a comment saying what this file is for');
  lines.forEach((text, i) => {
    for (const r of RESTRICTED) {
      if (test && r.only) continue;
      if (r.only && relative(ROOT, file) === r.only) continue;
      if (r.re.test(text)) report(file, i + 1, 'api', r.why);
    }
  });
  const from = { file, ...layerOf(file) };
  if (from.layer === 'feature-index') {
    const stray = lines.findIndex((l) => l.trim() && !/^\s*(\/\/|export (type )?\{[^}]*\} from '\.\/[^']+';$)/.test(l));
    if (stray >= 0) report(file, stray + 1, 'index', 'an index.ts only re-exports (`export { X } from \'./X\';`)');
    continue;
  }
  for (const imp of importsOf(text)) {
    const target = resolveImport(file, imp.spec);
    if (!target) {
      if (PURE.has(from.layer) && !test && !imp.typeOnly) report(file, imp.line, 'pure', `imports '${imp.spec}': ${FIX.pure}`);
      continue;
    }
    if (!target.startsWith(SRC)) continue;
    const to = { file: target, ...layerOf(target) };
    if (from.feature && to.feature && to.feature !== from.feature) {
      if (to.layer !== 'feature-index') {
        report(file, imp.line, 'index', `reaches into ${relative(SRC, target)}: ${FIX.index}`);
      } else if (!test) {
        if (!featureEdges.has(from.feature)) featureEdges.set(from.feature, new Set());
        featureEdges.get(from.feature).add(to.feature);
      }
      continue;
    }
    if (from.layer === 'app' && to.feature && to.layer !== 'feature-index') {
      report(file, imp.line, 'index', `reaches into ${relative(SRC, target)}: ${FIX.index}`);
      continue;
    }
    if (to.layer === 'test' && !test) {
      report(file, imp.line, 'layer', 'src/test is for tests only');
      continue;
    }
    if (!allowed(from, to)) {
      report(file, imp.line, 'layer', `${from.layer} may not import ${to.layer} (${relative(SRC, target)}): ${FIX.layer}`);
    }
  }
}

for (const feature of readdirSync(join(SRC, 'features'))) {
  if (!existsSync(join(SRC, 'features', feature, 'index.ts'))) {
    report(join(SRC, 'features', feature), 1, 'index', 'every feature has an index.ts: its public API');
  }
}

// Features depend on each other as a tree (a cycle means a shared part belongs lower down).
const state = new Map();
function visit(feature, path) {
  if (state.get(feature) === 'done') return;
  if (state.get(feature) === 'active') {
    const cycle = [...path.slice(path.indexOf(feature)), feature].join(' → ');
    report(join(SRC, 'features', feature, 'index.ts'), 1, 'cycle', `${cycle}: ${FIX.cycle}`);
    return;
  }
  state.set(feature, 'active');
  for (const next of featureEdges.get(feature) ?? []) visit(next, [...path, feature]);
  state.set(feature, 'done');
}
for (const feature of featureEdges.keys()) visit(feature, []);

if (problems.length) {
  console.error(`Architecture check: ${problems.length} problem(s)\n${problems.join('\n')}`);
  process.exit(1);
}
console.log(`Architecture check: ${files.length} files OK`);
