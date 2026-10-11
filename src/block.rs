use std::fmt;

/// Several lines of output that travel together.
///
/// Every multi-line component in tinsel returns a `Block` ([`Table`](crate::Table),
/// [`Tree`](crate::Tree), [`Card`](crate::Card), [`callout`](crate::callout), …), so they all
/// work the same way: print it, `println!` it, or take its lines and arrange them yourself.
///
/// ```
/// use tinsel::{Block, pairs};
///
/// let block: Block = pairs(&[("region", "eu-west-2"), ("memory", "512 MB")]);
/// block.print();                    // print it
/// println!("{block}");              // or format it
/// assert_eq!(block.len(), 2);       // or look inside
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    lines: Vec<String>,
}

impl Block {
    /// An empty block.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a line to the end.
    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    /// Add every line of another block to the end.
    pub fn extend(&mut self, other: Block) {
        self.lines.extend(other.lines);
    }

    /// The lines, still styled.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The lines, owned.
    pub fn into_lines(self) -> Vec<String> {
        self.lines
    }

    /// How many lines there are.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether there are no lines at all.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The text without any colour or styling, lines joined with `\n`. Handy in tests.
    pub fn plain(&self) -> String {
        crate::text::strip_ansi(&self.to_string())
    }

    /// Print every line to stdout. With [`Output::Json`](crate::Output::Json) the block is
    /// one `{"event":"block","lines":[…]}` event instead.
    pub fn print(&self) {
        if crate::output::json() {
            return self.emit();
        }
        for line in &self.lines {
            println!("{line}");
        }
    }

    /// Print every line to stderr (or, as JSON, to stdout like everything else).
    pub fn eprint(&self) {
        if crate::output::json() {
            return self.emit();
        }
        for line in &self.lines {
            eprintln!("{line}");
        }
    }
}

impl Block {
    /// Print the block through a pager (`$PAGER`, or `less`) if it's taller than the
    /// terminal, so it can be scrolled and searched; otherwise just print it. Colours are
    /// kept (`less -R`).
    ///
    /// ```no_run
    /// tinsel::markdown(include_str!("../CHANGELOG.md")).page();
    /// ```
    pub fn page(&self) {
        use std::io::{IsTerminal, Write};
        if crate::output::json() || !std::io::stdout().is_terminal() || self.len() < crate::term::height() {
            return self.print();
        }
        let pager = std::env::var("PAGER").ok().filter(|p| !p.trim().is_empty());
        let default = if cfg!(windows) { "more" } else { "less" };
        let command = pager.unwrap_or_else(|| default.to_string());
        let mut words = command.split_whitespace();
        let program = words.next().unwrap_or(default);
        let mut cmd = std::process::Command::new(program);
        cmd.args(words).stdin(std::process::Stdio::piped());
        if program.ends_with("less") && std::env::var_os("LESS").is_none() {
            // colours, quit if it fits after all, and leave the text on screen afterwards
            cmd.env("LESS", "-RFX");
        }
        let Ok(mut child) = cmd.spawn() else {
            return self.print();
        };
        if let Some(mut stdin) = child.stdin.take() {
            // the reader quitting early closes the pipe; that's fine
            let _ = stdin.write_all(format!("{self}\n").as_bytes());
        }
        let _ = child.wait();
    }

    fn emit(&self) {
        let lines: Vec<String> = self.lines.iter().map(|l| crate::text::strip_ansi(l)).collect();
        crate::output::emit("block", &[("lines", crate::output::Val::List(&lines))]);
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.lines.join("\n"))
    }
}

impl From<Vec<String>> for Block {
    fn from(lines: Vec<String>) -> Self {
        Block { lines }
    }
}

impl FromIterator<String> for Block {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Block {
            lines: iter.into_iter().collect(),
        }
    }
}

impl IntoIterator for Block {
    type Item = String;
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.lines.into_iter()
    }
}
