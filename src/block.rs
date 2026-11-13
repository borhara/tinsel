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

    /// Print every line to stdout (or the [`Target`](crate::Target)). With [`Output::Json`](crate::Output::Json) the block is
    /// one `{"event":"block","lines":[…]}` event instead.
    pub fn print(&self) {
        if crate::output::json() {
            return self.emit();
        }
        crate::output::write(&self.lines.iter().map(|l| format!("{l}\n")).collect::<String>());
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
        self.page_with(std::env::var("PAGER").ok(), std::env::var_os("LESS").is_some());
    }

    /// [`page`](Block::page) through `pager` (a command line), unless it's empty; `less_set`
    /// says whether `less` has options of its own already.
    fn page_with(&self, pager: Option<String>, less_set: bool) {
        use std::io::Write;
        let paged = crate::output::target() == crate::Target::Stdout && crate::output::is_terminal();
        if crate::output::json() || !paged || self.len() < crate::term::height() {
            return self.print();
        }
        let pager = pager.filter(|p| !p.trim().is_empty());
        let default = if cfg!(windows) { "more" } else { "less" };
        let command = pager.unwrap_or_else(|| default.to_string());
        let mut words = command.split_whitespace();
        let program = words.next().unwrap_or(default);
        let mut cmd = std::process::Command::new(program);
        cmd.args(words).stdin(std::process::Stdio::piped());
        if program.ends_with("less") && !less_set {
            // colours, quit if it fits after all, and leave the text on screen afterwards
            cmd.env("LESS", "-RFX");
        }
        let Ok(mut child) = cmd.spawn() else {
            return self.print();
        };
        // the reader quitting early closes the pipe; that's fine
        let _ = child
            .stdin
            .take()
            .map(|mut stdin| stdin.write_all(format!("{self}\n").as_bytes()));
        let _ = child.wait();
    }

    /// The block as a web page that looks just like it does in the terminal, colours and all,
    /// as [`Frame::html`](crate::app::Frame::html) draws a frame. Draw the block in colour
    /// (see [`set_color_mode`](crate::set_color_mode)) to see them.
    ///
    /// ```
    /// let page = tinsel::pairs(&[("url", "https://example.com/?a=1&b=2")]).html();
    /// assert!(page.contains("a=1&amp;b=2"));
    /// ```
    pub fn html(&self) -> String {
        self.frame().html()
    }

    /// The block as a `<pre>` element, styled inline so it can go anywhere in a page. See
    /// [`html`](Block::html).
    ///
    /// ```
    /// let pre = tinsel::Block::from(vec!["<b>".to_string()]).html_pre();
    /// assert!(pre.ends_with("&lt;b&gt;</pre>"));
    /// ```
    pub fn html_pre(&self) -> String {
        self.frame().html_pre()
    }

    /// The block drawn into a frame just big enough for it.
    fn frame(&self) -> crate::app::Frame {
        let width = self.lines.iter().map(|l| crate::text::width(l)).max().unwrap_or(0);
        let mut frame = crate::app::Frame::new(width, self.len());
        frame.lines(frame.area(), self);
        frame
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::output::{Output, capture, with_output, with_terminal};

    fn two() -> Block {
        Block::from(vec!["b-one".to_string(), "b-two".to_string()])
    }

    #[test]
    fn prints_as_lines_or_one_event() {
        assert!(capture(|| two().print()).contains("b-one\nb-two\n"));
        two().eprint();
        let json = capture(|| with_output(Output::Json, || two().print()));
        assert!(
            json.contains(r#"{"event":"block","lines":["b-one","b-two"]}"#),
            "{json}"
        );
        let json = capture(|| with_output(Output::Json, || two().eprint()));
        assert!(json.contains(r#""lines":["b-one","b-two"]"#), "{json}");
    }

    #[test]
    fn short_blocks_and_pipes_arent_paged() {
        assert!(capture(|| two().page()).contains("b-one\n"));
        assert!(capture(|| with_terminal(true, || two().page())).contains("b-one\n"));
        let json = capture(|| with_output(Output::Json, || two().page()));
        assert!(json.contains(r#""event":"block""#));
    }

    #[test]
    fn blocks_are_built_and_taken_apart() {
        let mut b = Block::new();
        assert!(b.is_empty());
        b.push("x");
        b.extend(two());
        assert_eq!(b.len(), 3);
        assert_eq!(b.clone().into_lines(), ["x", "b-one", "b-two"]);
        let back: Block = b.clone().into_iter().collect();
        assert_eq!(back, b);
    }

    #[test]
    fn tall_blocks_on_a_terminal_go_through_the_pager() {
        let tall: Block = (0..crate::term::height() + 1).map(|i| format!("tall-{i}")).collect();
        // a pager that reads nothing and quits
        let shown = capture(|| with_terminal(true, || tall.page_with(Some("true".into()), true)));
        assert!(cfg!(windows) || !shown.contains("tall-0"), "{shown}");
        // a pager that isn't there: printed after all
        let missing = Some("/nonexistent/less -S".into());
        assert!(capture(|| with_terminal(true, || tall.page_with(missing, false))).contains("tall-0\n"));
    }
}
