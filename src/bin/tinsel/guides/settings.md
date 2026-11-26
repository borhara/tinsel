# Settings and terminals

tinsel works most things out for itself. When it decides wrong, `tinsel doctor` says what
it decided and why, and warns about any setting it can't use, like a misspelt
`TINSEL_THEMES` or `TINSEL_OUTPUT=jsn`. `tinsel::doctor()` gives you the same report, for a
`--debug-terminal` flag of your own.

## The settings

| Setting | What it does |
| --- | --- |
| `TINSEL_THEME` | `light` or `dark`, instead of asking the terminal |
| `TINSEL_ACCENT` | `violet`, `blue`, `teal`, `orange`, or a hue in degrees |
| `TINSEL_OUTPUT` | `json` or `quiet`, for tools and scripts |
| `TINSEL_TARGET` | `stderr`, leaving stdout for data |
| `TINSEL_FPS` | the most frames a second, 1 to 240 |
| `TINSEL_GLYPHS` | `ascii`, for a terminal whose font lacks the symbols |
| `TINSEL_AMBIGUOUS` | `wide`, for a terminal that draws ambiguous characters two columns across |
| `TINSEL_IMAGES` | `kitty`, `iterm` or `none`, instead of guessing |
| `TINSEL_NO_FOCUS` | stops watching whether the window has focus |
| `TINSEL_NO_HISTORY` | stops steps remembering how long they took |

`NO_COLOR`, `FORCE_COLOR` and `CLICOLOR` are respected, and most settings have a
function to choose them in code, like `tinsel::set_theme` and `tinsel::set_glyphs`.

## Glyphs

Lines, blocks, dots and spinners are Unicode's, and plain ASCII on the Linux console,
whose font has few of them. Each symbol becomes one ASCII character, so nothing moves.

## Ambiguous characters

Unicode marks some characters, like `●`, `→` and the lines tinsel draws panels with, as
ambiguous: most terminals draw them one column wide, but one set up for Chinese, Japanese
or Korean can draw them two. With `TINSEL_AMBIGUOUS=wide`, text is measured as the
terminal draws it, and tinsel's own symbols go out in ASCII, so layouts hold.

## Focus

In terminals that report window focus, animation slows while you're away, a run of 10s or
more that finishes while you're away sends a notification, and coming back re-twinkles
what finished.
