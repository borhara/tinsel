# Introduction

Clean terminal output for Rust CLIs, with a little sparkle. Live step lists, spinners,
tables, trees, prompts and error reports that all look like they belong together.

Most CLIs bolt together a spinner crate, a table crate, a prompt crate and a colour crate,
and end up looking like four different programs. tinsel is one small library with one
visual language: pink accents, quiet greys, eased animation, and a twinkle when something
finishes.

```tinsel deploy
```

## What it does for you

- **It learns:** steps remember how long they took, so next run a spinner becomes a real estimate: `usually 42s · 18s left`
- **It knows if you're watching:** switch windows during a long run and you get a desktop notification when it ends; come back and whatever finished meanwhile twinkles
- **It fits your terminal:** detects light and dark backgrounds and redraws the palette to suit, or in another accent colour for your brand
- **It speaks JSON too:** one switch turns every step, message and error into JSON lines for tools, or quiets everything but problems
- **Zero dependencies:** nothing in your lockfile but tinsel
- **Behaves in CI:** no colour or animation when piped, `NO_COLOR` respected, plain `[ok] Build (1.2s)` lines in logs
- **Panics stay tidy:** a panic mid-run leaves the last frame on screen and prints a proper report, not a scrambled terminal

## Reading these guides

Every guide here is drawn by tinsel, and the examples run as you read them. Move with
`j` and `k`, open a guide with `enter`, jump between the list, the page and its headings
with `tab`, and search with `/`. `q` leaves.

The same guides print as plain text when tinsel's output isn't a terminal, so
`tinsel docs live | less` works too.
