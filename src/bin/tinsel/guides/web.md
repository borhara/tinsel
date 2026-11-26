# Apps in a browser

A full-screen app can run on a web page as it runs in a terminal, keys, mouse and all, with
the `web` feature. Build it for wasm, and tinsel's own script draws it in the page. These
docs are one, at tinsel.shiv.mov.

```toml
[lib]
# a .wasm file a page can call into, and the library your own binary uses as before
crate-type = ["cdylib", "rlib"]

[dependencies]
tinsel = { version = "1.2", features = ["web"] }
```

```rust
// src/lib.rs: the same App you'd give app::run
tinsel::web_app!(Dash { latency: vec![84.0, 90.0, 120.0, 340.0, 95.0] });
```

```bash
rustup target add wasm32-wasip1        # once
cargo build --release --target wasm32-wasip1
```

That makes `target/wasm32-wasip1/release/dash.wasm`. Nothing is added to your lockfile:
the feature is tinsel's own code, and the target is part of Rust.

## Putting it on your site

Two files go where your site serves static files (`public/` in most frameworks, or beside
`index.html`): the app's `.wasm`, and `tinsel.js`, the page's side. That's the text of
`tinsel::web::SCRIPT`, so a test can write it out, or copy `src/web.js` from tinsel's
repository.

```rust
#[test]
fn page_script() {
    std::fs::write("site/tinsel.js", tinsel::web::SCRIPT).unwrap();
}
```

Then a page gives it an element to fill, sized as the app should be:

```html
<div id="app" style="height: 100vh"></div>
<script type="module">
  import { run } from './tinsel.js';
  run(document.getElementById('app'), '/dash.wasm', { onQuit: () => history.back() });
</script>
```

It passes in the keys, clicks, the wheel and drags on a touch screen, and redraws only the
rows that changed. `light: true` draws in tinsel's light colours, and `onQuit` hears which
key quit the app (`"q"`, `"Escape"`), to go somewhere else or stay. It's all static files,
so any host serves it, as long as `.wasm` goes out as `application/wasm`, as most do. Build
and copy again after updating tinsel or the app.

## What carries over

Ticks, `animate` and `redraw_at`, `app::time` and `app::size` behave as in a terminal, and
ambient animation rests while the page is out of sight. `tinsel::web::touch()` says when
there's no keyboard, for leaving out the keys' hints. A page has no threads, files,
processes or clipboard, so no `Job` or `Waker`, and pictures aren't shown.
