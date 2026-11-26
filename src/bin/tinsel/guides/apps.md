# Full-screen apps

Dashboards, pickers, little toys. Write a struct, draw it in `view`, change it in
`event`; `app::run` takes over the screen and gives it back afterwards, even on a panic.
These docs are one.

```rust
use tinsel::app::{self, App, Frame, Panel, Plot, Stat, Size::*};

struct Dash { latency: Vec<f64> }

impl App for Dash {
    fn view(&self, frame: &mut Frame) {
        let [stat, chart] = frame.area().columns_gap(1, [Fixed(20), Fill(1)]);
        frame.render(stat, Panel::new("p95").body(Stat::new("latency", "84ms")));
        frame.render(chart, Panel::new("latency").body(Plot::new(&self.latency)).focused(true));
    }
}

app::run(Dash { latency: vec![84.0, 90.0, 120.0, 340.0, 95.0] })?;   // q or Esc quits
```

## What goes in a panel

Anything tinsel draws (tables, trees, cards, any `Block`) goes straight into a panel, next
to `List`, `Tabs`, `Plot`, `Stat`, `big` text, `hints` and twinkling `Sparkles`. `Grid`
is a table with a cursor, for lists you move through. Panels shimmer when focused, and
lists scroll themselves.

## Drawing only when needed

A frame is drawn when something changes, or when it asks: `frame.animate()` for every
frame while something moves, `redraw_in` and `redraw_at` for one later. A still app costs
nothing.

## Testing and screenshots

`app::snapshot(&app, 80, 24)` draws a frame without a terminal, for tests, and
`Frame::html()` turns one into a web page that looks just like it, in any font: lines and
blocks are drawn, so they join, and symbols a font lacks keep to their cells.

Whole programs fit too: tvim is a vim-like text editor built on tinsel's full-screen apps.
