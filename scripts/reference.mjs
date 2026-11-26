// Writes the API reference `tinsel docs` shows in its Reference tab, src/bin/tinsel/reference.md,
// from tinsel itself: rustdoc's JSON for every public item, each signature as the source has
// it, and each doc comment. A page for each module, and for each part of the crate's root (the
// file its items are written in), an item at a `##` heading and its methods, fields and
// variants under it. Nothing is written by hand, so a new item turns up the next time it runs.
//
//   node scripts/reference.mjs           write it
//   node scripts/reference.mjs --check   fail if it's out of date, as CI does
//
// rustdoc's JSON is only on nightly, and changes between them, so it's pinned here: NIGHTLY
// picks another. Needs `rustup toolchain install nightly-<date> --profile minimal`.

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const NIGHTLY = process.env.NIGHTLY ?? 'nightly-2026-11-25';
const FORMAT = 61;
const root = new URL('..', import.meta.url).pathname;
const OUT = `${root}src/bin/tinsel/reference.md`;

execFileSync(
  'cargo',
  [`+${NIGHTLY}`, 'rustdoc', '--lib', '--all-features', '-q', '--', '-Z', 'unstable-options', '--output-format', 'json'],
  { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] },
);
const json = JSON.parse(readFileSync(`${root}target/doc/tinsel.json`, 'utf8'));
if (json.format_version !== FORMAT) {
  throw new Error(`rustdoc's JSON is format ${json.format_version}, and this reads ${FORMAT}: pin NIGHTLY to the one it was written for, or update it`);
}
const index = json.index;
const kind = (item) => Object.keys(item.inner)[0];

// the source as written, by file
const files = new Map();
const source = (span) => {
  if (!files.has(span.filename)) files.set(span.filename, readFileSync(`${root}${span.filename}`, 'utf8').split('\n'));
  const lines = files.get(span.filename).slice(span.begin[0] - 1, span.end[0]);
  lines[lines.length - 1] = lines[lines.length - 1].slice(0, span.end[1]);
  lines[0] = lines[0].slice(span.begin[1] - 1);
  return lines;
};

// an item's signature, as the source has it: up to its body, without attributes, the lines
// after the first in from the left as far as the first is
function signature(item) {
  const lines = source(item.span).filter((l) => !l.trim().startsWith('#['));
  const text = lines.join('\n');
  let depth = 0;
  let end = text.length;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if ('(<['.includes(c)) depth++;
    else if (')>]'.includes(c) && text[i - 1] !== '-' && text[i - 1] !== '=') depth--;
    else if (depth <= 0 && (c === '{' || c === ';')) {
      end = i;
      break;
    }
  }
  const sig = text.slice(0, end).trimEnd();
  const indent = Math.min(
    ...sig
      .split('\n')
      .slice(1)
      .filter((l) => l.trim())
      .map((l) => l.match(/^ */)[0].length),
  );
  return sig
    .split('\n')
    .map((l, i) => (i && Number.isFinite(indent) ? l.slice(Math.max(0, indent - 4)) : l))
    .join('\n');
}

// a doc comment as the reader shows Markdown: rustdoc's hidden example lines left out, its
// links as their words, and its headings as bold lines, below the page's own
function docs(text) {
  if (!text) return '';
  const out = [];
  let fence = null;
  for (const line of text.split('\n')) {
    const open = line.match(/^\s*```(.*)$/);
    if (open && fence === null) {
      const lang = open[1].trim().split(',')[0];
      fence = ['', 'rust', 'no_run', 'ignore', 'should_panic', 'compile_fail', 'edition2024', 'edition2021'].includes(lang)
        ? 'rust'
        : lang;
      out.push('```' + fence);
      continue;
    }
    if (open && fence !== null) {
      fence = null;
      out.push('```');
      continue;
    }
    if (fence === 'rust' && /^\s*#(\s|$)/.test(line)) continue;
    if (fence !== null) {
      out.push(line);
      continue;
    }
    const heading = line.match(/^#+\s+(.*)$/);
    if (heading) {
      out.push(`**${heading[1]}**`);
      continue;
    }
    out.push(
      line
        .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
        .replace(/\[([^\]]+)\]\[[^\]]*\]/g, '$1')
        .replace(/\[(`[^`\]]+`)\](?![(\[])/g, '$1'),
    );
  }
  return out.join('\n').trim();
}

const block = (code) => '```rust\n' + code + '\n```';
const KIND = { struct: 'struct', enum: 'enum', trait: 'trait', function: 'fn', constant: 'const', static: 'static', type_alias: 'type', macro: 'macro', module: 'module' };

// an item, with its methods, fields or variants under it
function render(item, path) {
  const k = kind(item);
  const name = k === 'macro' ? `${item.name}!` : item.name;
  const out = [`## ${name}`, '', `\`${path}${name}\` · ${KIND[k] ?? k}`, ''];
  if (k === 'macro') out.push(block(`${item.name}!(…)`));
  else out.push(block(signature(item)));
  const d = docs(item.docs);
  if (d) out.push('', d);

  const sub = (title, members) => {
    const shown = members.filter(Boolean);
    if (!shown.length) return;
    out.push('', `**${title}**`);
    for (const m of shown) {
      out.push('', block(signature(m)));
      const md = docs(m.docs);
      if (md) out.push('', md);
    }
  };
  const inner = item.inner[k];
  if (k === 'struct' && inner.kind.plain) {
    sub('Fields', inner.kind.plain.fields.map((id) => index[id]).filter((f) => f?.visibility === 'public'));
  }
  if (k === 'enum') sub('Variants', inner.variants.map((id) => index[id]));
  if (k === 'trait') sub('Methods', inner.items.map((id) => index[id]));
  if (k === 'struct' || k === 'enum') {
    const methods = inner.impls
      .map((id) => index[id])
      .filter((i) => i && !i.inner.impl.trait && !i.inner.impl.blanket_impl)
      .flatMap((i) => i.inner.impl.items.map((id) => index[id]))
      .filter((m) => m?.visibility === 'public' && m.span);
    sub('Methods', methods);
  }
  return out.join('\n');
}

// The crate root's items by the file they're written in, put together as the README's "What's in
// it" groups them; a file not named here gets a page of its own, so nothing is ever left out.
const GROUPS = {
  'Live work': ['live', 'region', 'spin', 'widgets', 'lib'],
  Messages: ['message', 'notify', 'panic'],
  'Tables, trees and charts': ['table', 'tree', 'chart', 'block'],
  'Text, code and JSON': ['layout', 'json', 'help'],
  'Command lines': ['cli'],
  Settings: ['output', 'fps', 'glyphs', 'history', 'watch', 'term', 'doctor'],
};
const groupOf = (file) => Object.keys(GROUPS).find((g) => GROUPS[g].includes(file)) ?? file;
const fileOf = (item) => item.span.filename.replace(/^src\//, '').replace(/(\/mod)?\.rs$/, '').split('/')[0];

// the pages: those groups first, then each public module, a root item written in a module's
// file put with that module; each item once a page
const pages = new Map(Object.keys(GROUPS).map((g) => [g, { intro: '', items: [], seen: new Set() }]));
const page = (title) => {
  if (!pages.has(title)) pages.set(title, { intro: '', items: [], seen: new Set() });
  return pages.get(title);
};
const modules = new Set();
const add = (title, item, path) => {
  const p = page(title);
  if (p.seen.has(item.id)) return;
  p.seen.add(item.id);
  p.items.push(render(item, path));
};
function module(id, path) {
  const m = index[id];
  for (const child of m.inner.module.items.map((i) => index[i])) {
    if (!child || child.visibility !== 'public') continue;
    const k = kind(child);
    if (k === 'module') {
      if (child.name === 'prelude') continue;
      modules.add(child.name);
      page(path + child.name).intro = docs(child.docs);
      module(child.id, `${path}${child.name}::`);
      continue;
    }
    const target = k === 'use' ? child.inner.use.id != null && index[child.inner.use.id] : child;
    // everything public in the module a glob names, as if each were named
    if (k === 'use' && child.inner.use.is_glob) {
      if (target && kind(target) === 'module') module(target.id, path);
      continue;
    }
    if (!target || !target.span || kind(target) === 'module' || kind(target) === 'impl') continue;
    const item = k === 'use' ? { ...target, name: child.inner.use.name } : target;
    if (path !== 'tinsel::') add(path.slice(0, -2), item, path);
    else {
      const file = fileOf(target);
      add(modules.has(file) ? `tinsel::${file}` : groupOf(file), item, path);
    }
  }
}
// the modules' pages are made first, so a root item written in one's file knows it's there
for (const id of index[json.root].inner.module.items) {
  const m = index[id];
  if (m.visibility === 'public' && kind(m) === 'module' && m.name !== 'prelude') modules.add(m.name);
}
module(json.root, 'tinsel::');

const header = `<!-- written by scripts/reference.mjs from tinsel's own source: don't edit it by hand -->`;
const text = [
  header,
  ...[...pages]
    .filter(([, p]) => p.items.length)
    .map(([title, p]) => [`# ${title}`, p.intro, ...p.items].filter(Boolean).join('\n\n')),
].join('\n\n') + '\n';

if (process.argv.includes('--check')) {
  if (readFileSync(OUT, 'utf8') !== text) {
    console.error('src/bin/tinsel/reference.md is out of date: run node scripts/reference.mjs');
    process.exit(1);
  }
} else {
  writeFileSync(OUT, text);
  console.log(`${OUT.replace(root, '')}: ${pages.size} pages, ${Math.round(text.length / 1024)} KB`);
}
