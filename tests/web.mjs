// That the page's side, src/web.js, and an app's side, src/web.rs, still agree: every export
// the page calls is there, the keys are numbered as the app reads them, and the app asks for
// nothing of WASI the page doesn't answer. Runs the docs page's wasm, as CI builds it:
//
//   cargo build --example web_docs --features web --target wasm32-wasip1
//   node --test tests/web.mjs
//
// Node's own test runner, so it needs nothing installed. WASM=path picks another build.

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { KEYS, imports } from '../src/web.js';

const root = new URL('..', import.meta.url).pathname;
const built = ['debug', 'release'].map((p) => `${root}target/wasm32-wasip1/${p}/examples/web_docs.wasm`);
const path = process.env.WASM ?? built.find(existsSync);
assert.ok(path, `no wasm to run: build it first with\n  cargo build --example web_docs --features web --target wasm32-wasip1`);

const source = readFileSync(`${root}src/web.js`, 'utf8');
const module = new WebAssembly.Module(readFileSync(path));

// the docs started at 100 by 30, and what was asked of WASI that the page doesn't answer
async function start() {
  const unanswered = new Set();
  let app;
  const instance = await WebAssembly.instantiate(
    module,
    imports(() => app.memory, (name) => unanswered.add(name)),
  );
  app = instance.exports;
  app._initialize?.();
  app.tinsel_start(100, 30, 0, 0);
  const frame = () => {
    const ptr = app.tinsel_frame();
    return new TextDecoder().decode(new Uint8Array(app.memory.buffer, ptr, app.tinsel_frame_len()));
  };
  // the text of a frame, as a reader sees it
  const shown = () => frame().replace(/<[^>]*>/g, '');
  return { app, frame, shown, unanswered };
}

const typed = (app, text) => {
  for (const c of text) app.tinsel_key(0, c.codePointAt(0));
};

test('every export the page calls is there', () => {
  const called = new Set(source.match(/\bw\.tinsel_\w+/g).map((c) => c.slice(2)));
  const exported = new Set(WebAssembly.Module.exports(module).map((e) => e.name));
  for (const name of called) assert.ok(exported.has(name), `the page calls ${name}, which the app doesn't export`);
  assert.ok(called.size >= 10, `only found ${[...called]}`);
});

test('the app asks only for what the page answers', async () => {
  const { shown, app, unanswered } = await start();
  shown();
  typed(app, 'jjn');
  shown();
  assert.deepEqual([...unanswered], []);
});

test('the first frame is drawn, as a <pre> the page can put in place', async () => {
  const { app, frame } = await start();
  const html = frame();
  assert.equal(app.tinsel_changed(), 1);
  assert.match(html, /^<pre style=/);
  assert.match(html.replace(/<[^>]*>/g, ''), /tinsel docs/);
  // a line to a row, which the page puts in place a row at a time
  assert.equal(html.split('\n').length, 30);
});

test('the keys are numbered as the app reads them', async () => {
  const { app, shown } = await start();
  shown();
  // a search typed and applied, which takes characters, Enter and Escape
  typed(app, '/pic');
  app.tinsel_key(KEYS.Enter, 0);
  assert.match(shown(), /showing\s+pic/);
  app.tinsel_key(KEYS.Escape, 0);
  assert.doesNotMatch(shown(), /showing\s+pic/);
  // the arrows move through the guides, End to the last of them
  app.tinsel_key(KEYS.Tab, 0);
  app.tinsel_key(KEYS.End, 0);
  app.tinsel_key(KEYS.Enter, 0);
  assert.match(shown(), /Settings and terminals/);
});

test('q quits, as the page is told', async () => {
  const { app, shown } = await start();
  shown();
  assert.equal(app.tinsel_key(0, 'j'.codePointAt(0)), 0);
  assert.equal(app.tinsel_key(0, 'q'.codePointAt(0)), 1);
});
