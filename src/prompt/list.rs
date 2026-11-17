//! Choosing from a list: one, several, or from a dropdown.

use super::*;

/// How many rows a list shows at once, unless told otherwise.
pub(super) const VISIBLE: usize = 8;

/// The slice of a long list to show, keeping the cursor in view.
pub(super) fn window(len: usize, cursor: usize, rows: usize) -> std::ops::Range<usize> {
    let shown = len.min(rows);
    let top = cursor.saturating_sub(shown / 2).min(len - shown);
    top..top + shown
}

/// Whether `query`'s characters all appear in `option`, in order (ignoring case), and if so
/// where, so they can be highlighted.
pub(super) fn fuzzy(option: &str, query: &str) -> Option<Vec<usize>> {
    let mut hits = Vec::new();
    let mut want = query.chars().flat_map(char::to_lowercase).peekable();
    for (i, c) in option.chars().enumerate() {
        let Some(&w) = want.peek() else { break };
        if c.to_lowercase().eq(std::iter::once(w)) {
            hits.push(i);
            want.next();
        }
    }
    want.peek().is_none().then_some(hits)
}

/// One option in a list question.
#[derive(Default)]
pub(super) struct Choice {
    label: String,
    /// A heading drawn above this option, starting a group.
    heading: Option<String>,
    /// Why it can't be picked.
    disabled: Option<String>,
    note: Option<String>,
    badges: Vec<String>,
    dim: bool,
}

/// What every list question has: the question, its options and how they're shown.
pub(super) struct Choices {
    question: String,
    list: Vec<Choice>,
    /// A heading waiting for the next option.
    heading: Option<String>,
    hint: Option<String>,
    back: bool,
    rows: usize,
}

impl Choices {
    fn new(question: String) -> Self {
        Choices {
            question,
            list: Vec::new(),
            heading: None,
            hint: None,
            back: false,
            rows: VISIBLE,
        }
    }

    fn push(&mut self, label: String, disabled: Option<String>) {
        let heading = self.heading.take();
        self.list.push(Choice {
            label,
            heading,
            disabled,
            ..Choice::default()
        });
    }

    fn enabled(&self, i: usize) -> bool {
        self.list.get(i).is_some_and(|c| c.disabled.is_none())
    }

    /// The muted line of keys under the list.
    fn keys(&self, base: &str) -> String {
        keys(&format!("{base}{}", if self.back { " · ← back" } else { "" }))
    }
}

/// The builder methods every list question shares, for adding and decorating options.
macro_rules! list_builders {
    ($ty:ident) => {
        impl $ty<'_> {
            /// Add an option. Options are numbered from 0 in the order they're added,
            /// disabled ones included, and the answer is given as those numbers.
            #[must_use]
            pub fn option(mut self, label: impl Into<String>) -> Self {
                self.choices.push(label.into(), None);
                self
            }

            /// Add several options.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty),
                                "::new(\"Deploy to\").options([\"staging\", \"production\"]).ask()?;\n",
                                "# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            #[must_use]
            pub fn options<S: Into<String>>(mut self, labels: impl IntoIterator<Item = S>) -> Self {
                for label in labels {
                    self.choices.push(label.into(), None);
                }
                self
            }

            /// Add an option that can't be picked, drawn dim with the reason after it. The
            /// cursor skips over it, but it still takes up a number.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty), "::new(\"Deploy to\")\n",
                                "    .option(\"staging\")\n    .disabled(\"production\", \"frozen until Monday\")\n",
                                "    .ask()?;\n# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            #[must_use]
            pub fn disabled(mut self, label: impl Into<String>, reason: impl Into<String>) -> Self {
                self.choices.push(label.into(), Some(reason.into()));
                self
            }

            /// Start a group: a heading over the options added after it. It's not an option,
            /// so it can't be picked and takes up no number.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty), "::new(\"Service\")\n",
                                "    .heading(\"Frontend\")\n    .options([\"web\", \"docs\"])\n",
                                "    .heading(\"Backend\")\n    .options([\"api\", \"worker\"])\n",
                                "    .ask()?;\n# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            #[must_use]
            pub fn heading(mut self, title: impl Into<String>) -> Self {
                self.choices.heading = Some(title.into());
                self
            }

            /// Muted text lined up on the right of the option added last, such as when it
            /// was made.
            #[doc = concat!(
                                "```no_run\nlet answer = tinsel::prompt::", stringify!($ty), "::new(\"Version\")\n",
                                "    .option(\"v1.2.0\").note(\"3 days ago\").badge(\"latest\")\n",
                                "    .option(\"v1.1.0\").note(\"2 weeks ago\")\n",
                                "    .ask()?;\n# Ok::<(), tinsel::prompt::Error>(())\n```"
                            )]
            #[must_use]
            pub fn note(mut self, text: impl Into<String>) -> Self {
                if let Some(c) = self.choices.list.last_mut() {
                    c.note = Some(text.into());
                }
                self
            }

            /// A small badge after the option added last, such as `latest` or `current`.
            /// Call it again for another.
            #[must_use]
            pub fn badge(mut self, label: impl Into<String>) -> Self {
                if let Some(c) = self.choices.list.last_mut() {
                    c.badges.push(label.into());
                }
                self
            }

            /// Draw the option added last quieter than the rest, for ones that are less
            /// likely to be wanted (a pre-release, say). It can still be picked.
            #[must_use]
            pub fn dim(mut self) -> Self {
                if let Some(c) = self.choices.list.last_mut() {
                    c.dim = true;
                }
                self
            }

            /// A muted line under the question, to say more about what's being asked.
            #[must_use]
            pub fn hint(mut self, text: impl Into<String>) -> Self {
                self.choices.hint = Some(text.into());
                self
            }

            /// Let ← go back to the question before, as [`Error::Back`].
            #[must_use]
            pub fn back(mut self) -> Self {
                self.choices.back = true;
                self
            }

            /// How many rows to show at once (8 unless set); longer lists scroll, with a
            /// count of what's above and below.
            #[must_use]
            pub fn rows(mut self, rows: usize) -> Self {
                self.choices.rows = rows.max(1);
                self
            }
        }
    };
}

/// A row of a list as drawn.
pub(super) enum Row {
    /// A group heading, over the option with this number.
    Heading(usize),
    /// An option, by position in what's shown.
    Option(usize),
    /// Use what's been typed as the answer.
    Custom,
}

/// The options shown in a list question, narrowed by what's been typed in long lists.
pub(super) struct Picker<'a> {
    choices: &'a Choices,
    /// Lists that can be filtered by typing; others keep j/k for moving.
    filterable: bool,
    /// Whether what's typed can be the answer itself.
    custom: bool,
    filter: String,
    /// Indices into the options that match the filter, with the matched characters.
    shown: Vec<(usize, Vec<usize>)>,
    /// Position in `shown`, or `shown.len()` for the typed answer.
    cursor: usize,
}

impl<'a> Picker<'a> {
    fn new(choices: &'a Choices, filterable: bool, start: Option<usize>) -> Self {
        let mut picker = Picker {
            choices,
            filterable,
            custom: false,
            filter: String::new(),
            shown: Vec::new(),
            cursor: 0,
        };
        picker.refilter();
        if let Some(pos) = start.filter(|&i| choices.enabled(i)) {
            picker.cursor = pos;
        }
        picker
    }

    /// Whether the typed text is offered as an answer of its own.
    fn offers_custom(&self) -> bool {
        self.custom && !self.filter.is_empty() && !self.choices.list.iter().any(|c| c.label == self.filter)
    }

    /// How many positions the cursor can be in.
    fn len(&self) -> usize {
        self.shown.len() + self.offers_custom() as usize
    }

    fn selectable(&self, pos: usize) -> bool {
        match self.shown.get(pos) {
            Some((i, _)) => self.choices.enabled(*i),
            None => pos < self.len(),
        }
    }

    fn refilter(&mut self) {
        let keep = self.shown.get(self.cursor).map(|(i, _)| *i);
        self.shown = (0..self.choices.list.len())
            .filter_map(|i| fuzzy(&self.choices.list[i].label, &self.filter).map(|hits| (i, hits)))
            .collect();
        // stay on the same option if it's still there
        self.cursor = keep
            .and_then(|k| self.shown.iter().position(|(i, _)| *i == k))
            .filter(|&pos| self.selectable(pos))
            .or_else(|| (0..self.len()).find(|&pos| self.selectable(pos)))
            .unwrap_or(0);
    }

    fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.refilter();
    }

    fn clear_filter(&mut self) {
        self.filter.clear();
        self.refilter();
    }

    /// The option under the cursor, if it can be picked.
    fn current(&self) -> Option<usize> {
        self.shown
            .get(self.cursor)
            .map(|(i, _)| *i)
            .filter(|&i| self.choices.enabled(i))
    }

    /// Whether the cursor is on the typed answer.
    fn on_custom(&self) -> bool {
        self.offers_custom() && self.cursor == self.shown.len()
    }

    /// Move to the next option that can be picked, wrapping around.
    fn step(&mut self, down: bool) {
        let n = self.len();
        for k in 1..n {
            let pos = if down {
                (self.cursor + k) % n
            } else {
                (self.cursor + n - k) % n
            };
            if self.selectable(pos) {
                self.cursor = pos;
                return;
            }
        }
    }

    /// Move a page at a time, stopping at the ends.
    fn page(&mut self, down: bool) {
        let n = self.len();
        if n == 0 {
            return;
        }
        let target = if down {
            (self.cursor + self.choices.rows).min(n - 1)
        } else {
            self.cursor.saturating_sub(self.choices.rows)
        };
        // the nearest option that can be picked, between the cursor and the target
        let found = if down {
            (self.cursor..=target).rev().find(|&pos| self.selectable(pos))
        } else {
            (target..=self.cursor).find(|&pos| self.selectable(pos))
        };
        self.cursor = found.unwrap_or(self.cursor);
    }

    /// Handle a key that moves or filters; returns `false` for keys it doesn't use.
    fn key(&mut self, key: Key) -> bool {
        match key {
            Key::Up => self.step(false),
            Key::Down => self.step(true),
            Key::PageUp => self.page(false),
            Key::PageDown => self.page(true),
            Key::Char('k') if !self.filterable => self.step(false),
            Key::Char('j') if !self.filterable => self.step(true),
            Key::Char(c) if self.filterable => self.type_char(c),
            Key::Backspace if self.filterable => {
                self.filter.pop();
                self.refilter();
            }
            Key::Ctrl('u') if self.filterable => self.clear_filter(),
            _ => return false,
        }
        true
    }

    /// The question line, with the filter typed so far and how many options match it, then
    /// the hint, if there is one.
    fn top(&self) -> Vec<String> {
        let question = &self.choices.question;
        let mut lines = vec![if self.filter.is_empty() {
            ask(question)
        } else {
            format!(
                "{}  {}{}  {}",
                ask(question),
                paint(TEXT, &self.filter),
                paint(ROSE, "▏"),
                paint(MUTED, &format!("{} of {}", self.shown.len(), self.choices.list.len()))
            )
        }];
        lines.extend(self.choices.hint.as_deref().map(keys));
        lines
    }

    fn layout(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut after = 0;
        for (pos, (i, _)) in self.shown.iter().enumerate() {
            // only the nearest heading: groups in between have nothing shown
            if let Some(h) = (after..=*i).rev().find(|&h| self.choices.list[h].heading.is_some()) {
                rows.push(Row::Heading(h));
            }
            rows.push(Row::Option(pos));
            after = i + 1;
        }
        if self.offers_custom() {
            rows.push(Row::Custom);
        }
        rows
    }

    /// The visible rows, each option after `prefix(option, is cursor)`; with a count of
    /// options above and below when the list scrolls.
    fn rows(&self, prefix: &dyn Fn(Option<usize>, bool) -> String) -> Vec<String> {
        let layout = self.layout();
        if layout.is_empty() {
            return vec![format!("      {}", paint(MUTED, "no matches"))];
        }
        let at = layout
            .iter()
            .position(|r| match r {
                Row::Option(pos) => *pos == self.cursor,
                Row::Custom => self.on_custom(),
                Row::Heading(_) => false,
            })
            .unwrap_or(0);
        let range = window(layout.len(), at, self.choices.rows);
        let scrolls = layout.len() > self.choices.rows;
        let count = |rows: &[Row]| rows.iter().filter(|r| !matches!(r, Row::Heading(_))).count();
        let more = |n: usize, arrow: &str| {
            if n == 0 {
                String::new()
            } else {
                format!("      {}", paint(MUTED, &format!("{arrow} {n} more")))
            }
        };
        let (left, right) = self.widths();
        let mut lines = Vec::new();
        if scrolls {
            lines.push(more(count(&layout[..range.start]), "↑"));
        }
        for row in &layout[range.clone()] {
            lines.push(match row {
                Row::Heading(h) => {
                    let title = self.choices.list[*h].heading.as_deref().unwrap_or_default();
                    format!("    {}", bold(&paint(BLUSH, title)))
                }
                Row::Option(pos) => {
                    let (i, hits) = &self.shown[*pos];
                    let here = *pos == self.cursor;
                    format!("{}{}", prefix(Some(*i), here), self.body(*i, hits, here, left, right))
                }
                Row::Custom => {
                    let here = self.on_custom();
                    let typed = paint(if here { TEXT } else { SOFT }, &format!("“{}”", self.filter));
                    format!("{}{} {typed}", prefix(None, here), paint(MUTED, "use"))
                }
            });
        }
        if scrolls {
            lines.push(more(count(&layout[range.end..]), "↓"));
        }
        lines
    }

    /// How wide the label column (with badges and reasons) and the note column are, over
    /// every option so they don't shift as the list scrolls.
    fn widths(&self) -> (usize, usize) {
        let list = &self.choices.list;
        let left = list.iter().map(left_width).max().unwrap_or(0);
        let right = list
            .iter()
            .filter_map(|c| c.note.as_deref().map(text::width))
            .max()
            .unwrap_or(0);
        (left, right)
    }

    /// An option's label (matched letters picked out), badges, reason and note.
    fn body(&self, i: usize, hits: &[usize], here: bool, left: usize, right: usize) -> String {
        let c = &self.choices.list[i];
        let base = if c.disabled.is_some() || c.dim {
            MUTED
        } else if here {
            TEXT
        } else {
            SOFT
        };
        let mut s: String = c
            .label
            .chars()
            .enumerate()
            .map(|(ci, ch)| {
                let color = if hits.contains(&ci) && c.disabled.is_none() {
                    ROSE
                } else {
                    base
                };
                paint(color, ch.encode_utf8(&mut [0; 4]))
            })
            .collect();
        // the row under the cursor stands out as it does in app lists
        if here && base == TEXT {
            s = bold(&s);
        }
        for b in &c.badges {
            s += " ";
            s += &small_badge(b);
        }
        if let Some(why) = &c.disabled {
            s += "  ";
            s += &italic(&paint(MUTED, &format!("({why})")));
        }
        if let Some(note) = &c.note {
            s += &" ".repeat(left - left_width(c) + 2);
            s += &paint(MUTED, &text::pad_left(note, right));
        }
        s
    }
}

/// Columns taken by an option's label, badges and reason.
pub(super) fn left_width(c: &Choice) -> usize {
    text::width(&c.label)
        + c.badges.iter().map(|b| text::width(b) + 3).sum::<usize>()
        + c.disabled.as_deref().map_or(0, |why| text::width(why) + 4)
}

/// A badge in the accent, or `[label]` without colour, where a pill wouldn't show.
pub(super) fn small_badge(label: &str) -> String {
    if crate::style::color_mode() == crate::ColorMode::None {
        format!("[{label}]")
    } else {
        crate::badge(ROSE, label)
    }
}

/// The pointer column for lists that pick one.
pub(super) fn pointer(here: bool) -> String {
    if here {
        format!("    {} ", paint(ROSE, "›"))
    } else {
        "      ".into()
    }
}

/// One of a list, moved through with ↑↓ (or j/k). Returns the chosen index. In lists longer
/// than eight, typing narrows the list to options containing those letters in order, and
/// Esc clears what's typed before it cancels. [`Select`] has more options.
pub fn select(question: &str, options: &[&str]) -> Result<usize> {
    Select::new(question).options(options.iter().copied()).ask()
}

/// One of a list, with options: where to start, groups under headings, options that can't be
/// picked (and why), notes and badges, a hint, validation and going back. [`select`] is the
/// shortcut. Returns the number of the chosen option, counting from 0 in the order they
/// were added.
///
/// ↑↓ (or j/k) move, PgUp/PgDn move a page. Lists longer than [`rows`](Select::rows) filter
/// as you type, and Esc clears the filter before it cancels.
///
/// Without a terminal, the [`selected`](Select::selected) option is the answer, if there is one;
/// otherwise it's [`Error::NotATerminal`].
///
/// ```no_run
/// use tinsel::prompt::Select;
///
/// let target = Select::new("Deploy to")
///     .hint("production needs a second approval")
///     .heading("Shared")
///     .option("staging")
///     .option("production")
///     .disabled("legacy", "retired")
///     .heading("Yours")
///     .option("preview")
///     .selected(0)
///     .ask()?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub struct Select<'a> {
    choices: Choices,
    default: Option<usize>,
    validate: Option<Box<dyn Fn(usize) -> Checked + 'a>>,
}

list_builders!(Select);

impl std::fmt::Debug for Select<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Select")
            .field("question", &self.choices.question)
            .field("options", &self.choices.list.len())
            .field("selected", &self.default)
            .finish_non_exhaustive()
    }
}

impl<'a> Select<'a> {
    /// A question with one answer from a list.
    pub fn new(question: impl Into<String>) -> Self {
        Select {
            choices: Choices::new(question.into()),
            default: None,
            validate: None,
        }
    }

    /// Start on this option (by number), and answer with it when there's no terminal.
    #[must_use]
    pub fn selected(mut self, index: usize) -> Self {
        self.default = Some(index);
        self
    }

    /// Check the choice when Enter is pressed. An `Err` is shown under the question, which
    /// stays open.
    ///
    /// ```no_run
    /// let size = tinsel::prompt::Select::new("Size")
    ///     .options(["small", "large"])
    ///     .validate(|i| if i == 1 { Err("large needs a paid plan".into()) } else { Ok(()) })
    ///     .ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    #[must_use]
    pub fn validate(mut self, check: impl Fn(usize) -> std::result::Result<(), String> + 'a) -> Self {
        self.validate = Some(Box::new(check));
        self
    }

    fn check(&self, i: usize) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(i))
    }

    /// Ask the question. The answer is the chosen option's number, counted from 0 in the
    /// order options were added (headings don't count).
    pub fn ask(self) -> Result<usize> {
        if !interactive() {
            return self.unasked();
        }
        if !(0..self.choices.list.len()).any(|i| self.choices.enabled(i)) {
            return Err(Error::NoOptions);
        }
        self.run(&mut Ui::open()?)
    }

    /// The answer without a terminal: the default, if there is one.
    pub(super) fn unasked(&self) -> Result<usize> {
        let i = self
            .default
            .filter(|&i| self.choices.enabled(i))
            .ok_or(Error::NotATerminal)?;
        self.check(i).map_err(Error::Invalid)?;
        Ok(i)
    }

    pub(super) fn run(&self, ui: &mut Ui) -> Result<usize> {
        let question = &self.choices.question;
        let filterable = self.choices.list.len() > self.choices.rows;
        let mut picker = Picker::new(&self.choices, filterable, self.default);
        let hint = if filterable {
            "type to filter · ↑↓ move · enter choose"
        } else {
            "↑↓ move · enter choose"
        };
        let mut trouble: Option<String> = None;
        loop {
            let mut lines = picker.top();
            lines.extend(trouble.as_deref().map(problem));
            lines.extend(picker.rows(&|_, here| pointer(here)));
            lines.push(self.choices.keys(hint));
            ui.draw(&lines);
            match ui.key()? {
                Key::Enter | Key::Right => {
                    if let Some(i) = picker.current() {
                        match self.check(i) {
                            Ok(()) => {
                                ui.draw(&[answered(question, &self.choices.list[i].label)]);
                                return Ok(i);
                            }
                            Err(why) => trouble = Some(why),
                        }
                    }
                }
                Key::Left if self.choices.back => return Err(ui.back()),
                // Esc clears a filter first, and only cancels once there's nothing to clear
                Key::Esc if !picker.filter.is_empty() => picker.clear_filter(),
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
                key => {
                    if picker.key(key) {
                        trouble = None;
                    }
                }
            }
        }
    }
}

/// Several of a list: space toggles, `a` toggles all, enter confirms. Returns the chosen
/// indices in order. In lists longer than eight, typing filters and Ctrl-A toggles all of
/// what's shown. [`MultiSelect`] has more options.
pub fn multiselect(question: &str, options: &[&str], selected: &[usize]) -> Result<Vec<usize>> {
    need_terminal()?;
    MultiSelect::new(question)
        .options(options.iter().copied())
        .selected(selected)
        .ask()
}

/// Several of a list, with options: some ticked to start with, groups under headings,
/// options that can't be picked (and why), notes and badges, a hint, validation and going
/// back. [`multiselect`] is the shortcut. Returns the numbers of the chosen options in
/// order, counting from 0 in the order they were added.
///
/// Space toggles, `a` toggles all (Ctrl-A, all of what's shown, in lists that filter as you
/// type), Enter is done.
///
/// Without a terminal, the [`selected`](MultiSelect::selected) options are the answer, if any were
/// given; otherwise it's [`Error::NotATerminal`].
///
/// ```no_run
/// use tinsel::prompt::MultiSelect;
///
/// let services = MultiSelect::new("Restart")
///     .options(["api", "web", "worker"])
///     .selected(&[0])
///     .validate(|picked| if picked.is_empty() { Err("pick at least one".into()) } else { Ok(()) })
///     .ask()?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub struct MultiSelect<'a> {
    choices: Choices,
    default: Option<Vec<usize>>,
    validate: Option<ManyCheck<'a>>,
}

list_builders!(MultiSelect);

impl std::fmt::Debug for MultiSelect<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiSelect")
            .field("question", &self.choices.question)
            .field("options", &self.choices.list.len())
            .field("selected", &self.default)
            .finish_non_exhaustive()
    }
}

impl<'a> MultiSelect<'a> {
    /// A question with any number of answers from a list.
    pub fn new(question: impl Into<String>) -> Self {
        MultiSelect {
            choices: Choices::new(question.into()),
            default: None,
            validate: None,
        }
    }

    /// Tick these options (by number) to start with, and answer with them when there's no
    /// terminal. Options that can't be picked stay unticked.
    #[must_use]
    pub fn selected(mut self, indices: &[usize]) -> Self {
        self.default = Some(indices.to_vec());
        self
    }

    /// Check the choices when Enter is pressed. An `Err` is shown under the question, which
    /// stays open.
    #[must_use]
    pub fn validate(mut self, check: impl Fn(&[usize]) -> std::result::Result<(), String> + 'a) -> Self {
        self.validate = Some(Box::new(check));
        self
    }

    fn check(&self, picked: &[usize]) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(picked))
    }

    fn ticked(&self) -> Vec<bool> {
        let default = self.default.as_deref().unwrap_or_default();
        (0..self.choices.list.len())
            .map(|i| default.contains(&i) && self.choices.enabled(i))
            .collect()
    }

    /// Ask the question. The answer is the numbers of the ticked options, in order, counted
    /// as [`Select::ask`] counts them.
    pub fn ask(self) -> Result<Vec<usize>> {
        if !interactive() {
            return self.unasked();
        }
        if self.choices.list.is_empty() {
            return self.check(&[]).map(|()| Vec::new()).map_err(|_| Error::NoOptions);
        }
        self.run(&mut Ui::open()?)
    }

    /// The answer without a terminal: the default, if one was given.
    pub(super) fn unasked(&self) -> Result<Vec<usize>> {
        if self.default.is_none() {
            return Err(Error::NotATerminal);
        }
        let ticked = self.ticked();
        let chosen: Vec<usize> = (0..ticked.len()).filter(|&i| ticked[i]).collect();
        self.check(&chosen).map_err(Error::Invalid)?;
        Ok(chosen)
    }

    pub(super) fn run(&self, ui: &mut Ui) -> Result<Vec<usize>> {
        let question = &self.choices.question;
        let filterable = self.choices.list.len() > self.choices.rows;
        let mut picker = Picker::new(&self.choices, filterable, None);
        let mut picked = self.ticked();
        let hint = if filterable {
            "type to filter · ↑↓ move · space toggle · ctrl-a all · enter done"
        } else {
            "↑↓ move · space toggle · a all · enter done"
        };
        let mut trouble: Option<String> = None;
        loop {
            let mut lines = picker.top();
            lines.extend(trouble.as_deref().map(problem));
            lines.extend(picker.rows(&|i, here| {
                let i = i.unwrap_or_default();
                let mark = if picked[i] {
                    paint(ROSE, "◉")
                } else if self.choices.enabled(i) {
                    paint(MUTED, "○")
                } else {
                    paint(MUTED, "–")
                };
                let pointer = if here { paint(ROSE, "›") } else { " ".into() };
                format!("    {pointer} {mark} ")
            }));
            lines.push(self.choices.keys(hint));
            ui.draw(&lines);
            let toggle_all = |picked: &mut Vec<bool>, among: Vec<usize>| {
                let among: Vec<usize> = among.into_iter().filter(|&i| self.choices.enabled(i)).collect();
                let all = among.iter().all(|&i| picked[i]);
                among.iter().for_each(|&i| picked[i] = !all);
            };
            match ui.key()? {
                Key::Space => {
                    if let Some(i) = picker.current() {
                        picked[i] = !picked[i];
                    }
                    trouble = None;
                }
                Key::Char('a') if !picker.filterable => {
                    toggle_all(&mut picked, (0..self.choices.list.len()).collect());
                    trouble = None;
                }
                Key::Ctrl('a') => {
                    toggle_all(&mut picked, picker.shown.iter().map(|(i, _)| *i).collect());
                    trouble = None;
                }
                Key::Enter | Key::Right => {
                    let chosen: Vec<usize> = (0..picked.len()).filter(|&i| picked[i]).collect();
                    match self.check(&chosen) {
                        Ok(()) => {
                            let names: Vec<&str> =
                                chosen.iter().map(|&i| self.choices.list[i].label.as_str()).collect();
                            ui.draw(&[answered(question, &summary(&names))]);
                            return Ok(chosen);
                        }
                        Err(why) => trouble = Some(why),
                    }
                }
                Key::Left if self.choices.back => return Err(ui.back()),
                Key::Esc if !picker.filter.is_empty() => picker.clear_filter(),
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
                key => {
                    picker.key(key);
                }
            }
        }
    }
}

/// The chosen names for an answered line: as many whole names as fit, then how many more.
pub(super) fn summary(names: &[&str]) -> String {
    let Some(first) = names.first() else {
        return "none".into();
    };
    let mut shown = text::truncate(first, 40);
    for (k, name) in names.iter().enumerate().skip(1) {
        let rest = names.len() - k - 1;
        let tail = if rest > 0 { format!(" and {rest} more").len() } else { 0 };
        if text::width(&shown) + 2 + text::width(name) + tail > 40 {
            return format!("{shown} and {} more", names.len() - k);
        }
        shown = format!("{shown}, {name}");
    }
    shown
}

/// One of a long list, such as versions or branches, as a dropdown: a window of rows that
/// scrolls, filtering as you type, with notes lined up on the right, badges, and options
/// drawn quieter than the rest. With [`allow_custom`](Dropdown::allow_custom), what's typed
/// can be the answer too. Returns the chosen option's label, or the typed answer.
///
/// Without a terminal, the [`selected`](Dropdown::selected) option is the answer, if there is one;
/// otherwise it's [`Error::NotATerminal`].
///
/// ```no_run
/// use tinsel::prompt::Dropdown;
///
/// let version = Dropdown::new("Roll back to")
///     .option("v1.2.0").note("3 days ago").badge("latest").badge("current")
///     .option("v1.2.0-rc.1").note("5 days ago").dim()
///     .option("v1.1.0").note("2 weeks ago")
///     .option("v1.0.0").note("2 months ago")
///     .selected(2)
///     .allow_custom()
///     .ask()?;
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
pub struct Dropdown<'a> {
    choices: Choices,
    default: Option<usize>,
    custom: bool,
    validate: Option<Validator<'a>>,
}

list_builders!(Dropdown);

impl std::fmt::Debug for Dropdown<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dropdown")
            .field("question", &self.choices.question)
            .field("options", &self.choices.list.len())
            .field("selected", &self.default)
            .finish_non_exhaustive()
    }
}

impl<'a> Dropdown<'a> {
    /// A question with one answer from a long list.
    pub fn new(question: impl Into<String>) -> Self {
        Dropdown {
            choices: Choices::new(question.into()),
            default: None,
            custom: false,
            validate: None,
        }
    }

    /// Start on this option (by number), and answer with it when there's no terminal.
    #[must_use]
    pub fn selected(mut self, index: usize) -> Self {
        self.default = Some(index);
        self
    }

    /// Let the answer be something that isn't in the list: once something's typed that
    /// isn't an option, it's offered at the end of the list to choose.
    #[must_use]
    pub fn allow_custom(mut self) -> Self {
        self.custom = true;
        self
    }

    /// Check the answer when Enter is pressed, whether an option or typed. An `Err` is
    /// shown under the question, which stays open.
    ///
    /// ```no_run
    /// let tag = tinsel::prompt::Dropdown::new("Tag")
    ///     .options(["v1.1.0", "v1.0.0"])
    ///     .allow_custom()
    ///     .validate(|s| if s.starts_with('v') { Ok(()) } else { Err("tags start with v".into()) })
    ///     .ask()?;
    /// # Ok::<(), tinsel::prompt::Error>(())
    /// ```
    #[must_use]
    pub fn validate(mut self, check: impl Fn(&str) -> std::result::Result<(), String> + 'a) -> Self {
        self.validate = Some(Box::new(check));
        self
    }

    fn check(&self, value: &str) -> std::result::Result<(), String> {
        self.validate.as_ref().map_or(Ok(()), |check| check(value))
    }

    /// Ask the question. The answer is the chosen option's text rather than its number,
    /// since with [`allow_custom`](Dropdown::allow_custom) it may be typed and have none.
    pub fn ask(self) -> Result<String> {
        if !interactive() {
            return self.unasked();
        }
        if !self.custom && !(0..self.choices.list.len()).any(|i| self.choices.enabled(i)) {
            return Err(Error::NoOptions);
        }
        self.run(&mut Ui::open()?)
    }

    /// The answer without a terminal: the default, if there is one.
    pub(super) fn unasked(&self) -> Result<String> {
        let i = self
            .default
            .filter(|&i| self.choices.enabled(i))
            .ok_or(Error::NotATerminal)?;
        let value = self.choices.list[i].label.clone();
        self.check(&value).map_err(Error::Invalid)?;
        Ok(value)
    }

    pub(super) fn run(&self, ui: &mut Ui) -> Result<String> {
        let question = &self.choices.question;
        let mut picker = Picker::new(&self.choices, true, self.default);
        picker.custom = self.custom;
        let hint = if self.custom {
            "type to filter or enter your own · ↑↓ move · enter choose"
        } else {
            "type to filter · ↑↓ move · enter choose"
        };
        let mut trouble: Option<String> = None;
        loop {
            let mut lines = picker.top();
            lines.extend(trouble.as_deref().map(problem));
            lines.extend(picker.rows(&|_, here| pointer(here)));
            lines.push(self.choices.keys(hint));
            ui.draw(&lines);
            match ui.key()? {
                Key::Enter | Key::Right => {
                    let value = match picker.current() {
                        Some(i) => self.choices.list[i].label.clone(),
                        None if picker.on_custom() => picker.filter.clone(),
                        None => continue,
                    };
                    match self.check(&value) {
                        Ok(()) => {
                            ui.draw(&[answered(question, &value)]);
                            return Ok(value);
                        }
                        Err(why) => trouble = Some(why),
                    }
                }
                Key::Left if self.choices.back => return Err(ui.back()),
                Key::Space => {
                    picker.type_char(' ');
                    trouble = None;
                }
                Key::Esc if !picker.filter.is_empty() => picker.clear_filter(),
                Key::Esc | Key::Interrupt => return Err(cancel(&mut ui.frame, question)),
                key => {
                    if picker.key(key) {
                        trouble = None;
                    }
                }
            }
        }
    }
}
