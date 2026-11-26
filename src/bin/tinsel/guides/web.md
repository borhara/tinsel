# Apps in a browser

A full-screen app can run on a web page as it runs in a terminal, keys, mouse and all, with
the `web` feature. Build it for wasm, and tinsel's own script draws it in the page. These
docs are one, at tinsel.shiv.mov.

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
tinsel = { version = "1.2", features = ["web"] }
```

```rust
// src/lib.rs: the same App you'd give app::run
tinsel::web_app!(Dash { latency: vec![84.0, 90.0, 120.0, 340.0, 95.0] });
```

```bash
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
```

## On the page

`tinsel::web::SCRIPT` is the page's side: write it next to the page as `tinsel.js` (from a
test, say, or copy `src/web.js` from tinsel's repository) and give it an element to fill.

```html
<div id="app" style="height: 100vh"></div>
<script type="module">
  import { run } from './tinsel.js';
  run(document.getElementById('app'), 'dash.wasm', { onQuit: () => history.back() });
</script>
```

It passes in the keys, clicks, the wheel and drags on a touch screen, and redraws only the
rows that changed. `light: true` draws in tinsel's light colours.

## What carries over

Ticks, `animate` and `redraw_at`, `app::time` and `app::size` behave as in a terminal, and
ambient animation rests while the page is out of sight. `tinsel::web::touch()` says when
there's no keyboard, for leaving out the keys' hints. A page has no threads, files,
processes or clipboard, so no `Job` or `Waker`, and pictures aren't shown.
