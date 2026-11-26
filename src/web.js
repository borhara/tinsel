// tinsel's web page side: runs an app built with tinsel's `web` feature and `web_app!` in a
// page. The keys, the mouse, the element's size and whether the page is in view go in, and
// each frame comes out as tinsel's own HTML (Frame::html_pre), put in place a row at a time.
// What the app's exports take is in tinsel's src/web.rs.
//
//   import { run } from './tinsel.js';
//   run(document.getElementById('app'), 'my_app.wasm', { onQuit: (key) => history.back() });

/**
 * The keys with names, by the browser's name for them, as src/web.rs numbers them; a
 * character is 0 (1 with Ctrl, 2 with Alt), and Shift-Tab 5.
 */
export const KEYS = {
  Enter: 3,
  Tab: 4,
  Backspace: 6,
  Delete: 7,
  Escape: 8,
  ArrowUp: 9,
  ArrowDown: 10,
  ArrowLeft: 11,
  ArrowRight: 12,
  Home: 13,
  End: 14,
  PageUp: 15,
  PageDown: 16,
};

/**
 * The little of WASI that tinsel's std needs, for an app's `memory`: a clock, randomness for
 * its hash maps, an empty environment, and stderr to the console. Anything else (files) isn't
 * there, and is told to `unanswered` by name.
 *
 * @param {() => WebAssembly.Memory} memory
 * @param {(name: string) => void} [unanswered]
 */
export function imports(memory, unanswered = () => {}) {
  const view = () => new DataView(memory().buffer);
  const wasi = {
    clock_time_get(_id, _precision, out) {
      view().setBigUint64(out, BigInt(Math.round(performance.now() * 1e6)), true);
      return 0;
    },
    random_get(ptr, len) {
      crypto.getRandomValues(new Uint8Array(memory().buffer, ptr, len));
      return 0;
    },
    environ_sizes_get(count, size) {
      view().setUint32(count, 0, true);
      view().setUint32(size, 0, true);
      return 0;
    },
    environ_get: () => 0,
    fd_write(_fd, iovs, count, written) {
      const decoder = new TextDecoder();
      let total = 0;
      for (let i = 0; i < count; i++) {
        const ptr = view().getUint32(iovs + i * 8, true);
        const len = view().getUint32(iovs + i * 8 + 4, true);
        console.warn(decoder.decode(new Uint8Array(memory().buffer, ptr, len)));
        total += len;
      }
      view().setUint32(written, total, true);
      return 0;
    },
    proc_exit(code) {
      throw new Error(`the app exited with ${code}`);
    },
  };
  const ERRNO_NOSYS = 52;
  const answer = (t, name) =>
    t[name] ??
    (() => {
      unanswered(name);
      return ERRNO_NOSYS;
    });
  return { wasi_snapshot_preview1: new Proxy(wasi, { get: answer }) };
}

/**
 * Run the app in `wasm` (a URL, a Response or the bytes) in `element`, which it fills, and
 * follows as it changes size. Gives back `stop()`, which ends the app and empties the element.
 *
 * @param {HTMLElement} element
 * @param {string | URL | Response | Promise<Response> | BufferSource} wasm
 * @param {{ light?: boolean, fontSize?: () => number, onQuit?: (key: string) => void }} [options]
 *   `light` for tinsel's light colours, dark otherwise; `fontSize` in pixels, asked again as
 *   the window changes size (11 on a narrow window, 14 otherwise); `onQuit` when the app quits,
 *   with the browser's name for the key it quit on ("q", "Escape"), or "mouse"
 */
export async function run(element, wasm, options = {}) {
  const { light = false, fontSize = () => (innerWidth < 700 ? 11 : 14), onQuit = () => {} } = options;
  // every listener goes when it's stopped
  const stopping = new AbortController();
  const on = { signal: stopping.signal };
  const decoder = new TextDecoder();
  let w;
  const text = (ptr, len) => decoder.decode(new Uint8Array(w.memory.buffer, ptr, len));

  // The size of a cell, in the font tinsel's <pre> uses.
  let cell = { w: 8.4, h: 17.5, size: 14 };
  function measure() {
    const size = fontSize();
    const probe = document.createElement('pre');
    probe.style.cssText = `position:absolute;visibility:hidden;margin:0;font-family:ui-monospace,monospace;line-height:1.25;font-size:${size}px`;
    probe.textContent = 'x'.repeat(100);
    document.body.append(probe);
    const box = probe.getBoundingClientRect();
    probe.remove();
    cell = { w: box.width / 100, h: box.height, size };
  }

  // How many cells fit inside the element's padding.
  function cells() {
    measure();
    const css = getComputedStyle(element);
    const width = element.clientWidth - parseFloat(css.paddingLeft) - parseFloat(css.paddingRight);
    const height = element.clientHeight - parseFloat(css.paddingTop) - parseFloat(css.paddingBottom);
    return [Math.max(1, Math.floor(width / cell.w)), Math.max(1, Math.floor(height / cell.h))];
  }

  let broken = false;
  function fail(e) {
    broken = true;
    const why = w?.tinsel_panic_len() ? text(w.tinsel_panic(), w.tinsel_panic_len()) : String(e);
    const p = document.createElement('p');
    p.style.cssText = 'margin:0;font:14px/1.5 ui-monospace,monospace;opacity:0.7';
    p.textContent = `the app stopped: ${why}`;
    element.replaceChildren(p);
  }

  // Calls into the app, which stop for good if it panics.
  function call(f) {
    if (broken) return 0;
    try {
      return f();
    } catch (e) {
      fail(e);
      return 0;
    }
  }

  // The frame a row at a time: tinsel's <pre> has a line for each row, with no span open
  // across one, so each row is its own element, and only the rows that differ from the last
  // frame are replaced, as a panel's running light changes a row or two at a time.
  let opening = '';
  let rows = [];
  let shown = [];
  let size = 0;
  function show(html) {
    const start = html.indexOf('>') + 1;
    const lines = html.slice(start, html.lastIndexOf('</pre>')).split('\n');
    if (html.slice(0, start) !== opening || lines.length !== rows.length || cell.size !== size) {
      opening = html.slice(0, start);
      size = cell.size;
      element.innerHTML = `${opening}${lines.map((l) => `<span>${l}</span>`).join('\n')}</pre>`;
      const pre = element.firstElementChild;
      // filling the element, rather than sitting in it as a picture, its background the
      // element's so the edge doesn't show
      element.style.background = pre.style.background;
      pre.style.display = 'block';
      pre.style.padding = '0';
      pre.style.fontSize = `${cell.size}px`;
      pre.style.userSelect = 'none';
      shown = [...pre.children];
    } else {
      lines.forEach((l, i) => {
        if (l !== rows[i]) shown[i].innerHTML = l;
      });
    }
    rows = lines;
  }

  // Each of the browser's frames asks for the app's; the app draws only when something
  // happened or a frame it asked for is due.
  function draw() {
    if (broken) return;
    requestAnimationFrame(draw);
    call(() => {
      const ptr = w.tinsel_frame();
      if (w.tinsel_changed()) show(text(ptr, w.tinsel_frame_len()));
    });
  }

  element.tabIndex = 0;
  element.style.outline = 'none';
  element.addEventListener(
    'keydown',
    (e) => {
      // the browser's own shortcuts stay the browser's
      if (e.metaKey) return;
      let kind;
      let ch = 0;
      if (e.key === 'Tab' && e.shiftKey) kind = 5;
      else if (e.key in KEYS) kind = KEYS[e.key];
      else if ([...e.key].length === 1) {
        kind = e.ctrlKey ? 1 : e.altKey ? 2 : 0;
        ch = (e.ctrlKey ? e.key.toLowerCase() : e.key).codePointAt(0);
      }
      if (kind === undefined) return;
      e.preventDefault();
      if (call(() => w.tinsel_key(kind, ch))) onQuit(e.key);
    },
    on,
  );

  // The cell under the pointer.
  function at(e) {
    const box = (element.firstElementChild ?? element).getBoundingClientRect();
    return [
      Math.max(0, Math.floor((e.clientX - box.left) / cell.w)),
      Math.max(0, Math.floor((e.clientY - box.top) / cell.h)),
    ];
  }
  const mouse = (kind, [x, y]) => {
    if (call(() => w.tinsel_mouse(kind, x, y))) onQuit('mouse');
  };

  element.addEventListener(
    'mousedown',
    (e) => {
      if (e.button !== 0) return;
      element.focus();
      mouse(0, at(e));
    },
    on,
  );
  addEventListener('mouseup', (e) => mouse(1, at(e)), on);
  element.addEventListener(
    'mousemove',
    (e) => {
      if (e.buttons & 1) mouse(2, at(e));
    },
    on,
  );

  // The wheel a row at a time, however finely the trackpad reports it.
  let wheel = 0;
  element.addEventListener(
    'wheel',
    (e) => {
      e.preventDefault();
      wheel += e.deltaMode === 1 ? e.deltaY * cell.h : e.deltaY;
      const where = at(e);
      while (Math.abs(wheel) >= cell.h) {
        const down = wheel > 0;
        wheel -= down ? cell.h : -cell.h;
        mouse(down ? 4 : 3, where);
      }
    },
    { passive: false, signal: stopping.signal },
  );

  // On a touch screen: a drag scrolls as the wheel does, and a tap clicks.
  element.style.touchAction = 'none';
  let touch = null;
  element.addEventListener(
    'touchstart',
    (e) => {
      const t = e.touches[0];
      touch = { y: t.clientY, moved: false };
    },
    on,
  );
  element.addEventListener(
    'touchmove',
    (e) => {
      if (!touch) return;
      const t = e.touches[0];
      const where = at(t);
      while (Math.abs(touch.y - t.clientY) >= cell.h) {
        const down = touch.y > t.clientY;
        touch.y += down ? -cell.h : cell.h;
        touch.moved = true;
        mouse(down ? 4 : 3, where);
      }
    },
    on,
  );
  element.addEventListener(
    'touchend',
    (e) => {
      if (touch && !touch.moved) {
        const where = at(e.changedTouches[0]);
        mouse(0, where);
        mouse(1, where);
      }
      touch = null;
    },
    on,
  );

  // the element's size, not the window's, which it can change without: grown to fill a page,
  // say, or put somewhere else
  const resized = new ResizeObserver(() => w && call(() => w.tinsel_resize(...cells())));
  resized.observe(element);
  // out of view as a terminal in the background, so ambient animation rests
  document.addEventListener('visibilitychange', () => call(() => w.tinsel_focus(document.hidden ? 0 : 1)), on);

  const stop = () => {
    broken = true;
    stopping.abort();
    resized.disconnect();
    element.replaceChildren();
  };

  const wasiImports = imports(() => w.memory);
  try {
    const source = typeof wasm === 'string' || wasm instanceof URL ? fetch(wasm) : wasm;
    const bytes = async () => {
      const got = await source;
      return got instanceof Response ? got.clone().arrayBuffer() : got;
    };
    const resolved = await source;
    const { instance } =
      resolved instanceof Response
        ? await WebAssembly.instantiateStreaming(resolved.clone(), wasiImports).catch(async () =>
            WebAssembly.instantiate(await bytes(), wasiImports),
          )
        : await WebAssembly.instantiate(resolved, wasiImports);
    w = instance.exports;
    w._initialize?.();
    const [width, height] = cells();
    w.tinsel_start(width, height, light ? 1 : 0, matchMedia('(pointer: coarse)').matches ? 1 : 0);
    element.focus({ preventScroll: true });
    requestAnimationFrame(draw);
  } catch (e) {
    fail(e);
  }
  return { stop };
}
