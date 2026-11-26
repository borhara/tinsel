# Pictures

Pictures show at the terminal's full resolution, through kitty's graphics protocol or
iTerm2's inline images. A terminal that speaks neither can't show them at all: there's no
blocky stand-in, so ask first, and say so instead.

```rust
use tinsel::image::{Image, support};

let image = Image::new(std::fs::read("shot.png")?)?;
match support().check(&image) {
    Ok(_) => frame.image(area, &image),
    Err(why) => { frame.print(area, 0, &why); }
}
```

The picture is fitted to the area, keeping its shape and never made bigger than it is,
and taken away once a frame no longer has it. It sits above the text, so nothing else
should be drawn where it goes.

## Which terminals

kitty, Ghostty and Konsole take kitty's protocol; iTerm2 and WezTerm take iTerm2's. tmux
and screen hold the escapes back unless set up to pass them on, so they count as can't.
`TINSEL_IMAGES=kitty`, `iterm` or `none` decides it instead.

## JPEG and GIF in kitty

iTerm2 reads PNG, JPEG and GIF itself. kitty's protocol takes PNG as it is, and JPEG and
GIF as pixels, which the `decode` feature decodes in tinsel:

```toml
tinsel = { version = "1.2", features = ["decode"] }
```

Photos stand the way they were taken, from their EXIF orientation. A big one is decoded
on another thread: `image.pending()` says so until it's ready, so you can draw something
else there meanwhile, something that moves, so the next frame comes.
