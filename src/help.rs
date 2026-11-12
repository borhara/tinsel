//! `--help` output in tinsel's style, with a short entrance on a terminal.

use std::fmt;
use std::io::Write;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::block::Block;
use crate::highlight::{self, Lang};
use crate::style::palette::{BLUSH, MUTED, ROSE, SHINE, SOFT, TEXT};
use crate::style::{RAMP, Rgb, blink, bold, mix, paint, sample};
use crate::term::content_width;
use crate::widgets::{Spinner, noise, pop};
use crate::{output, term, text};

/// Where faded-out colours start from: the dark background tinsel's palette is designed on
/// (redrawn light on light terminals, like everything else).
const BACKGROUND: Rgb = Rgb(24, 20, 23);
/// Draws one line of help, given how to tint its colours while it fades in, and whether it
/// has fully arrived.
type LineBuilder<'a> = dyn Fn(&dyn Fn(Rgb) -> Rgb, bool) -> String + 'a;

/// Descriptions start on their own line when the left column would be wider than this.
const MAX_LEFT: usize = 30;

#[derive(Clone, Debug)]
enum Kind {
    Items,
    Examples,
}

#[derive(Clone, Debug)]
struct Group {
    title: String,
    kind: Kind,
    rows: Vec<(String, String)>,
}

/// A program's help: name, what it does, usage, then groups of commands, options and
/// examples, all aligned and wrapped to the terminal.
///
/// [`animate`](Help::animate) prints it with a short entrance (lines fade in one after
/// another while the name shimmers) when stdout is a terminal, and plainly otherwise. Either
/// way, the ✦ beside the name is left [blinking](crate::blink), so it keeps twinkling in
/// terminals that support it after your program has exited.
///
/// ```
/// use tinsel::Help;
///
/// Help::new("deploy", "ship an app to the cloud")
///     .version("1.4.0")
///     .usage("deploy [OPTIONS] <APP>")
///     .command("status", "what's running where")
///     .command("logs [--since 5m]", "recent logs")
///     .flag("--region <NAME>", "where to deploy (default: eu-west-2)")
///     .flag("-q, --quiet", "only print problems")
///     .example("deploy my-app --region us-east-1", "ship to the US east region")
///     .footer("docs: https://example.com/deploy")
///     .print();
/// ```
#[derive(Clone, Debug, Default)]
pub struct Help {
    name: String,
    about: String,
    version: Option<String>,
    usage: Vec<String>,
    groups: Vec<Group>,
    footer: Option<String>,
}

impl Help {
    /// Help for `name`, which does `about`.
    pub fn new(name: impl Into<String>, about: impl Into<String>) -> Self {
        Help {
            name: name.into(),
            about: about.into(),
            ..Self::default()
        }
    }

    /// The version, shown beside the name.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// A usage line, like `deploy [OPTIONS] <APP>`. Call again for more than one.
    pub fn usage(mut self, line: impl Into<String>) -> Self {
        self.usage.push(line.into());
        self
    }

    /// Start a new group of items under `title`. [`item`](Help::item) adds to it.
    pub fn group(mut self, title: impl Into<String>) -> Self {
        self.groups.push(Group {
            title: title.into(),
            kind: Kind::Items,
            rows: Vec::new(),
        });
        self
    }

    /// Add a row to the latest group: what to type on the left (`<args>` and `[optional]`
    /// parts are dimmed), what it does on the right.
    pub fn item(mut self, left: impl Into<String>, right: impl Into<String>) -> Self {
        if self.groups.is_empty() {
            self = self.group("");
        }
        if let Some(g) = self.groups.last_mut() {
            g.rows.push((left.into(), right.into()));
        }
        self
    }

    fn add_to(mut self, title: &str, kind: Kind, left: String, right: String) -> Self {
        match self.groups.iter_mut().find(|g| g.title == title) {
            Some(g) => g.rows.push((left, right)),
            None => self.groups.push(Group {
                title: title.into(),
                kind,
                rows: vec![(left, right)],
            }),
        }
        self
    }

    /// Add a subcommand, to the "commands" group.
    pub fn command(self, name: impl Into<String>, about: impl Into<String>) -> Self {
        self.add_to("commands", Kind::Items, name.into(), about.into())
    }

    /// Add a flag or option, to the "options" group.
    pub fn flag(self, flag: impl Into<String>, about: impl Into<String>) -> Self {
        self.add_to("options", Kind::Items, flag.into(), about.into())
    }

    /// Add an example command line and what it does, to the "examples" group.
    pub fn example(self, command: impl Into<String>, about: impl Into<String>) -> Self {
        self.add_to("examples", Kind::Examples, command.into(), about.into())
    }

    /// A closing line: where the docs are, how to report bugs.
    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.footer = Some(text.into());
        self
    }

    /// The help as lines.
    pub fn lines(&self) -> Block {
        self.render(&|_| 1.0, None).into()
    }

    /// Print the help.
    pub fn print(&self) {
        self.lines().print();
    }

    /// Print the help with a short entrance on a terminal (about a second); just print it
    /// anywhere else.
    pub fn animate(&self) {
        if output::output() != output::Output::Pretty || !output::is_terminal() {
            return self.print();
        }
        crate::style::theme();
        let settled = self.render(&|_| 1.0, None);
        // redrawing in place only works for what fits on screen: help taller than that
        // animates its top and prints the rest underneath once it's settled
        let room = term::height().saturating_sub(1).max(1).min(settled.len());
        let rows = room;
        // later lines follow quickly, so long help doesn't take long to arrive
        let stagger = (0.9 / rows as f32).min(0.035);
        let fade = 0.24;
        let reveal = |t: f32| move |i: usize| ease((t - 0.12 - i as f32 * stagger) / fade);
        let settle = 0.12 + rows as f32 * stagger + fade;
        let end = settle + 0.5;

        let prev = term::guard_interrupt();
        let mut out = output::writer();
        let _ = write!(out, "\x1b[?25l\x1b[?7l");
        let (start, mut drawn) = (Instant::now(), 0);
        loop {
            let t = start.elapsed().as_secs_f32();
            let done = t >= end;
            let mut lines = if done {
                settled.clone()
            } else {
                self.render(&reveal(t), Some((t, t - settle)))
            };
            lines.truncate(room);
            let mut buf = if drawn > 0 {
                format!("\x1b[{drawn}F")
            } else {
                String::new()
            };
            for line in &lines {
                buf += line;
                buf += "\x1b[K\n";
            }
            drawn = lines.len();
            if done {
                for line in &settled[room..] {
                    buf += line;
                    buf += "\x1b[K\n";
                }
            }
            let _ = out.write_all(buf.as_bytes());
            let _ = out.flush();
            if done {
                break;
            }
            sleep(Duration::from_millis(16));
        }
        let _ = write!(out, "\x1b[?7h\x1b[?25h");
        let _ = out.flush();
        term::restore_interrupt(prev);
    }

    /// The help with line `i` revealed `shown(i)` of the way (0..=1). `anim` is the time into
    /// the entrance and since everything arrived, while it's playing.
    fn render(&self, shown: &dyn Fn(usize) -> f32, anim: Option<(f32, f32)>) -> Vec<String> {
        let width = content_width();
        let mut lines: Vec<String> = Vec::new();
        // each line arrives faded and a little to the right, then settles
        let mut push = |build: &LineBuilder| {
            let f = shown(lines.len());
            let line = if f <= 0.0 {
                String::new()
            } else {
                let tint = move |c: Rgb| mix(BACKGROUND, c, f);
                let slide = ((1.0 - f) * 4.0).round() as usize;
                format!("{}{}", " ".repeat(slide), build(&tint, f >= 1.0))
            };
            lines.push(line);
        };

        // the title: a mark, the name, the version, and glitter that twinkles and fades away
        push(&|_, _| String::new());
        push(&|tint, _| {
            let (mark, name) = match anim {
                Some((t, since)) if since < 0.0 => (
                    Spinner::Sparkle.frame(t).to_string(),
                    crate::style::gradient(&self.name, &RAMP, t * 0.9),
                ),
                Some((_, since)) => {
                    let m = pop(since).map_or_else(|| paint(ROSE, "✦"), |l| l.to_string());
                    (format!(" {m} "), bold(&paint(ROSE, &self.name)))
                }
                // the settled mark blinks, which the terminal keeps doing after the program exits
                None => (
                    format!(" {} ", blink(&paint(ROSE, "✦"))),
                    bold(&paint(tint(ROSE), &self.name)),
                ),
            };
            let version = self
                .version
                .as_deref()
                .map(|v| format!("  {}", paint(tint(MUTED), v)))
                .unwrap_or_default();
            let glitter = match anim {
                Some((t, since)) => {
                    let left = 1.0 - ease((since + 0.4) / 0.6);
                    let tick = (t * 18.0) as u32;
                    let stars: String = (0..14)
                        .map(|i| {
                            let v = noise(i, tick) * left;
                            let glyph = match v {
                                v if v > 0.82 => "✦",
                                v if v > 0.68 => "✧",
                                v if v > 0.55 => "⋆",
                                v if v > 0.45 => "·",
                                _ => " ",
                            };
                            paint(sample(&[BLUSH, SHINE], v), glyph)
                        })
                        .collect();
                    format!("  {stars}")
                }
                None => String::new(),
            };
            format!("  {mark} {name}{version}{glitter}")
        });
        for line in text::wrap(&self.about, width.saturating_sub(7)) {
            push(&|tint, _| format!("       {}", paint(tint(SOFT), &line)));
        }

        if !self.usage.is_empty() {
            push(&|_, _| String::new());
            for (n, u) in self.usage.iter().enumerate() {
                push(&|tint, _| {
                    let label = if n == 0 { "usage" } else { "" };
                    format!(
                        "  {}  {}",
                        paint(tint(BLUSH), &format!("{label:<5}")),
                        colour_args(u, tint)
                    )
                });
            }
        }

        let left_w = self
            .groups
            .iter()
            .filter(|g| matches!(g.kind, Kind::Items))
            .flat_map(|g| g.rows.iter().map(|(l, _)| text::width(l)))
            .filter(|&w| w <= MAX_LEFT)
            .max()
            .unwrap_or(0);
        for group in &self.groups {
            push(&|_, _| String::new());
            if !group.title.is_empty() {
                push(&|tint, _| format!("  {}", bold(&paint(tint(BLUSH), &group.title))));
            }
            for (left, right) in &group.rows {
                match group.kind {
                    Kind::Items => {
                        let lw = text::width(left);
                        let desc_w = width.saturating_sub(4 + left_w + 2).max(20);
                        let desc = text::wrap(right, desc_w);
                        let hang = " ".repeat(4 + left_w + 2);
                        if lw > left_w {
                            // too wide to sit beside its description
                            push(&|tint, _| format!("    {}", colour_args(left, tint)));
                            for d in &desc {
                                push(&|tint, _| format!("{hang}{}", paint(tint(SOFT), d)));
                            }
                            continue;
                        }
                        for (n, d) in desc.iter().enumerate() {
                            push(&|tint, _| {
                                if n == 0 {
                                    let pad = " ".repeat(left_w - lw + 2);
                                    format!("    {}{pad}{}", colour_args(left, tint), paint(tint(SOFT), d))
                                } else {
                                    format!("{hang}{}", paint(tint(SOFT), d))
                                }
                            });
                        }
                    }
                    Kind::Examples => {
                        push(&|tint, settled| {
                            // fully arrived examples get shell colouring; fading ones stay plain
                            let cmd = if settled {
                                highlight::line(left, Lang::Shell, false)
                            } else {
                                paint(tint(TEXT), left)
                            };
                            format!("    {} {cmd}", paint(tint(MUTED), "$"))
                        });
                        for d in text::wrap(right, width.saturating_sub(6)) {
                            push(&|tint, _| format!("      {}", paint(tint(SOFT), &d)));
                        }
                    }
                }
            }
        }

        if let Some(footer) = &self.footer {
            push(&|_, _| String::new());
            push(&|tint, _| format!("  {}  {}", paint(tint(MUTED), "→"), paint(tint(SOFT), footer)));
        }
        push(&|_, _| String::new());
        lines
    }
}

/// A command line with its parts coloured: the command and subcommands in the accent,
/// flags bright, `<values>` and `[optional]` parts quiet.
fn colour_args(s: &str, tint: &dyn Fn(Rgb) -> Rgb) -> String {
    let mut depth = 0i32;
    let mut out = String::new();
    for (i, word) in s.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let opens = word.starts_with(['<', '[']);
        let color = if depth > 0 || opens {
            MUTED
        } else if word.starts_with('-') {
            TEXT
        } else {
            ROSE
        };
        depth += word.matches(['<', '[']).count() as i32 - word.matches(['>', ']']).count() as i32;
        out += &paint(tint(color), word);
    }
    out
}

/// Smooth start and end, for 0..1 (clamped).
fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl fmt::Display for Help {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lines().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_groups_and_examples() {
        crate::set_color_mode(crate::ColorMode::None);
        let help = Help::new("app", "does things")
            .version("1.0")
            .usage("app <CMD>")
            .command("run [ARGS]", "run it")
            .flag("-q, --quiet", "say less")
            .example("app run x", "runs x");
        assert_eq!(
            help.lines().plain(),
            [
                "",
                "   ✦  app  1.0",
                "       does things",
                "",
                "  usage  app <CMD>",
                "",
                "  commands",
                "    run [ARGS]   run it",
                "",
                "  options",
                "    -q, --quiet  say less",
                "",
                "  examples",
                "    $ app run x",
                "      runs x",
                "",
            ]
            .join("\n")
        );
    }

    #[test]
    fn faded_lines_keep_their_text() {
        crate::set_color_mode(crate::ColorMode::None);
        let help = Help::new("app", "does things").command("run", "run it");
        let settled = help.render(&|_| 1.0, None);
        let partway = help.render(&|i| if i < 3 { 1.0 } else { 0.0 }, Some((0.3, -0.5)));
        assert_eq!(settled.len(), partway.len());
        assert_eq!(text::strip_ansi(&settled[2]), text::strip_ansi(&partway[2]));
        assert!(partway[5].is_empty());
    }
}
