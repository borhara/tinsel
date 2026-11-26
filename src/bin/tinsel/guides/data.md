# Tables, trees and charts

## Tables

Numbers right-align and sort by their value, units and all:

```rust
use tinsel::{Border, Table, badge, palette};

Table::new(["service", "status", "p95"])
    .row(["api".to_string(), badge(palette::SAGE, "healthy"), "84ms".into()])
    .row(["auth".to_string(), badge(palette::AMBER, "slow"), "1.2s".into()])
    .sort_by_desc(2)           // by value: 1.2s before 84ms
    .border(Border::Rounded)
    .print();
```

```tinsel table
```

Tables wrap long cells, take footers, and highlight rows. `Tree` draws nested things,
`pairs` lines up keys and values, and `columns` sets blocks side by side.

## Charts

`sparkline`, `bars`, `meter`, `histogram` and `heatmap`, all in text:

```tinsel charts
```

## Documents

`tinsel::markdown(changelog).page()` draws Markdown (headings, lists, tables, code coloured
by language) and pages it if it's long; these guides are drawn with it.
`tinsel::json(body)?` indents and colours JSON.
