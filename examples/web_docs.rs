//! `tinsel docs` on a web page, the one at tinsel.shiv.mov: the reader the binary opens, run
//! with `web_app!`. Built for wasm:
//!
//!   cargo build --release --example web_docs --features web --target wasm32-wasip1
//!
//! and run in a page by `tinsel::web::SCRIPT`, from target/wasm32-wasip1/release/examples/.

#[path = "../src/bin/tinsel/demos.rs"]
mod demos;
#[path = "../src/bin/tinsel/docs.rs"]
#[allow(dead_code)]
mod docs;

// what docs::run calls for a guide that isn't there, which the page never asks for
fn usage_error(problem: &str) -> ! {
    panic!("{problem}")
}

tinsel::web_app!({
    let mut docs = docs::Docs::new(docs::Tab::Guides, 0);
    // a touch screen has no keys to be told about, and on a page it isn't plain that a
    // terminal library drew it
    docs.hints = !tinsel::web::touch();
    docs.credit = true;
    docs
});
