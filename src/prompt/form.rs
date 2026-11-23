//! Several questions asked as one: back and forth between them, then a look over the answers.

use super::*;

/// One question in a [`Form`]: any of the builders, which turn into one with `into()`.
#[non_exhaustive]
pub enum Question<'a> {
    /// Yes or no.
    Confirm(Confirm),
    /// A line of text.
    Input(Input<'a>),
    /// One of a list.
    Select(Select<'a>),
    /// Several of a list.
    MultiSelect(MultiSelect<'a>),
    /// One of a long list, or an answer typed in.
    Dropdown(Dropdown<'a>),
}

impl std::fmt::Debug for Question<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Question::Confirm(q) => q.fmt(f),
            Question::Input(q) => q.fmt(f),
            Question::Select(q) => q.fmt(f),
            Question::MultiSelect(q) => q.fmt(f),
            Question::Dropdown(q) => q.fmt(f),
        }
    }
}

impl From<Confirm> for Question<'_> {
    fn from(q: Confirm) -> Self {
        Question::Confirm(q)
    }
}

impl<'a> From<Input<'a>> for Question<'a> {
    fn from(q: Input<'a>) -> Self {
        Question::Input(q)
    }
}

impl<'a> From<Select<'a>> for Question<'a> {
    fn from(q: Select<'a>) -> Self {
        Question::Select(q)
    }
}

impl<'a> From<MultiSelect<'a>> for Question<'a> {
    fn from(q: MultiSelect<'a>) -> Self {
        Question::MultiSelect(q)
    }
}

impl<'a> From<Dropdown<'a>> for Question<'a> {
    fn from(q: Dropdown<'a>) -> Self {
        Question::Dropdown(q)
    }
}

/// What a question was answered with.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Answer {
    Yes(bool),
    Text(String),
    Choice(usize),
    Choices(Vec<usize>),
}

impl Question<'_> {
    fn question(&self) -> &str {
        match self {
            Question::Confirm(q) => &q.question,
            Question::Input(q) => &q.question,
            Question::Select(q) => &q.choices.question,
            Question::MultiSelect(q) => &q.choices.question,
            Question::Dropdown(q) => &q.choices.question,
        }
    }

    /// Whether there's anything to choose, for lists.
    fn answerable(&self) -> bool {
        match self {
            Question::Select(q) => q.choices.any_enabled(),
            Question::Dropdown(q) => q.custom || q.choices.any_enabled(),
            _ => true,
        }
    }

    /// Ask it on `ui`, starting from the answer it was given last time, if any. Returns the
    /// answer, and how it's shown.
    fn run(&mut self, ui: &mut Ui, back: bool, last: Option<&Answer>) -> Result<(Answer, String)> {
        match self {
            Question::Confirm(q) => {
                q.back = back;
                if let Some(Answer::Yes(yes)) = last {
                    q.default = *yes;
                }
                let yes = q.run(ui)?;
                Ok((Answer::Yes(yes), if yes { "yes" } else { "no" }.into()))
            }
            Question::Input(q) => {
                q.back = back;
                if let Some(Answer::Text(text)) = last {
                    q.initial = Some(text.clone());
                }
                let text = q.run(ui)?;
                let shown = if q.secret {
                    "•".repeat(text.chars().count())
                } else {
                    text.clone()
                };
                Ok((Answer::Text(text), shown))
            }
            Question::Select(q) => {
                q.choices.back = back;
                if let Some(Answer::Choice(i)) = last {
                    q.default = Some(*i);
                }
                let i = q.run(ui)?;
                Ok((Answer::Choice(i), q.choices.list[i].label.clone()))
            }
            Question::MultiSelect(q) => {
                q.choices.back = back;
                if let Some(Answer::Choices(picked)) = last {
                    q.default = Some(picked.clone());
                }
                let picked = q.run(ui)?;
                let names: Vec<&str> = picked.iter().map(|&i| q.choices.list[i].label.as_str()).collect();
                let shown = summary(&names);
                Ok((Answer::Choices(picked), shown))
            }
            Question::Dropdown(q) => {
                q.choices.back = back;
                if let Some(Answer::Text(text)) = last {
                    q.default = q.choices.list.iter().position(|c| &c.label == text);
                }
                let text = q.run(ui)?;
                Ok((Answer::Text(text.clone()), text))
            }
        }
    }

    /// The answer without a terminal, as the question asked on its own would find it.
    fn unasked(&self) -> Result<Answer> {
        match self {
            Question::Confirm(q) => q.unasked().map(Answer::Yes),
            Question::Input(q) => q.unasked().map(Answer::Text),
            Question::Select(q) => q.unasked().map(Answer::Choice),
            Question::MultiSelect(q) => q.unasked().map(Answer::Choices),
            Question::Dropdown(q) => q.unasked().map(Answer::Text),
        }
    }
}

/// Questions asked one after another as a group: ← goes back to the one before (with its
/// answer still there to change), and once they're all answered, a last question shows the
/// answers and offers to change any of them before going on.
///
/// Each question is given a name to find its answer by. Without a terminal the questions are
/// answered as they would be on their own (lines read from stdin, or what's `selected`), and
/// there's nothing to look over.
///
/// ```no_run
/// use tinsel::prompt::{Confirm, Form, Input, MultiSelect, Select};
///
/// let answers = Form::new()
///     .question("name", Input::new("Project name").default("my-app"))
///     .question("target", Select::new("Deploy to").options(["staging", "production"]))
///     .question("services", MultiSelect::new("Services").options(["api", "web", "worker"]))
///     .question("tag", Confirm::new("Tag the release?").default(true))
///     .ask()?;
///
/// let name = answers.text("name");
/// let target = answers.choice("target");
/// let services = answers.choices("services");
/// let tag = answers.yes("tag");
/// # Ok::<(), tinsel::prompt::Error>(())
/// ```
///
/// ```text
///   ●  Project name  my-app
///   ●  Deploy to  production
///   ●  Services  api, worker
///   ●  Tag the release?  yes
///   ?  All good?
///     › yes, carry on
///     change
///       Project name                     my-app
///       Deploy to                    production
///       …
/// ```
#[derive(Debug, Default)]
pub struct Form<'a> {
    questions: Vec<(String, Question<'a>)>,
}

impl<'a> Form<'a> {
    /// A form with no questions yet.
    pub fn new() -> Self {
        Form::default()
    }

    /// Add a question, and the name its answer is found by in the [`Answers`]. Its own
    /// `.back()` needn't be set: every question after the first can go back.
    #[must_use]
    pub fn question(mut self, name: impl Into<String>, question: impl Into<Question<'a>>) -> Self {
        self.questions.push((name.into(), question.into()));
        self
    }

    /// Ask the questions, then offer to change any answer before returning them all.
    ///
    /// # Errors
    ///
    /// [`Error::Cancelled`] on Ctrl-C or Esc at any question, [`Error::NoOptions`] when a list has
    /// nothing to choose, and otherwise whatever a question asked on its own would return
    /// without a terminal ([`Error::NotATerminal`], [`Error::Invalid`]) or when the terminal can't
    /// be used ([`Error::Io`]).
    pub fn ask(mut self) -> Result<Answers> {
        if !interactive() {
            let answers = self.questions.iter().map(|(name, q)| Ok((name.clone(), q.unasked()?)));
            return Ok(Answers {
                list: answers.collect::<Result<_>>()?,
            });
        }
        if self.questions.is_empty() {
            return Ok(Answers { list: Vec::new() });
        }
        if !self.questions.iter().all(|(_, q)| q.answerable()) {
            return Err(Error::NoOptions);
        }
        let mut ui = Ui::open()?;
        let result = self.run(&mut ui);
        // what's left on screen is the answers alone, or the question that was cancelled
        if result.is_ok() {
            ui.draw(&[]);
        }
        result
    }

    fn run(&mut self, ui: &mut Ui) -> Result<Answers> {
        let count = self.questions.len();
        let mut answers: Vec<Option<Answer>> = vec![None; count];
        // the answered lines, and each answer as it's shown, one per question
        let mut lines: Vec<String> = vec![String::new(); count];
        let mut shown: Vec<String> = vec![String::new(); count];
        let mut step = 0;
        // which question is being changed from the look over the answers, if one is
        let mut changing: Option<usize> = None;
        loop {
            let asking = changing.unwrap_or(step);
            if asking < count {
                // everything answered so far stays on screen above, except the one being asked
                ui.frame.above = (0..count)
                    .filter(|&i| i != asking && answers[i].is_some() && (changing.is_some() || i < asking))
                    .map(|i| lines[i].clone())
                    .collect();
                let back = changing.is_none() && asking > 0;
                let (_, question) = &mut self.questions[asking];
                match question.run(ui, back, answers[asking].as_ref()) {
                    Ok((answer, text)) => {
                        lines[asking] = ui.frame.lines.last().cloned().unwrap_or_default();
                        answers[asking] = Some(answer);
                        shown[asking] = text;
                        match changing.take() {
                            Some(_) => step = count,
                            None => step += 1,
                        }
                    }
                    Err(Error::Back) => step -= 1,
                    Err(e) => return Err(e),
                }
                continue;
            }
            // all answered: look them over
            ui.frame.above = lines.clone();
            let mut review = Select::new("All good?").option("yes, carry on").heading("change");
            for ((_, q), answer) in self.questions.iter().zip(&shown) {
                review = review.option(q.question()).note(text::truncate(answer, 30));
            }
            // room for every answer and the heading over them, so none scroll out of sight
            let review = review.rows(count + 2).back();
            match review.run(ui) {
                Ok(0) => break,
                Ok(n) => changing = Some(n - 1),
                Err(Error::Back) => step = count - 1,
                Err(e) => return Err(e),
            }
        }
        ui.frame.above = lines;
        let list = self
            .questions
            .iter()
            .map(|(name, _)| name.clone())
            .zip(answers.into_iter().flatten());
        Ok(Answers { list: list.collect() })
    }
}

/// The answers to a [`Form`], by the name each question was given.
///
/// Asking for a name the form doesn't have, or for the wrong kind of answer (`text` for a
/// yes or no question, say), panics: it's a mistake in the code, not something the person
/// answering could have done.
#[derive(Clone, Debug)]
pub struct Answers {
    list: Vec<(String, Answer)>,
}

impl Answers {
    fn get(&self, name: &str) -> &Answer {
        match self.list.iter().find(|(n, _)| n == name) {
            Some((_, answer)) => answer,
            None => panic!("the form has no question called {name:?}"),
        }
    }

    /// A [`Confirm`]'s answer.
    pub fn yes(&self, name: &str) -> bool {
        match self.get(name) {
            Answer::Yes(yes) => *yes,
            other => panic!("{name:?} wasn't a yes or no question: {other:?}"),
        }
    }

    /// An [`Input`]'s or a [`Dropdown`]'s answer.
    pub fn text(&self, name: &str) -> &str {
        match self.get(name) {
            Answer::Text(text) => text,
            other => panic!("{name:?} wasn't answered with text: {other:?}"),
        }
    }

    /// A [`Select`]'s answer: the number of the option chosen.
    pub fn choice(&self, name: &str) -> usize {
        match self.get(name) {
            Answer::Choice(i) => *i,
            other => panic!("{name:?} wasn't a Select: {other:?}"),
        }
    }

    /// A [`MultiSelect`]'s answer: the numbers of the options ticked, in order.
    pub fn choices(&self, name: &str) -> &[usize] {
        match self.get(name) {
            Answer::Choices(picked) => picked,
            other => panic!("{name:?} wasn't a MultiSelect: {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Key;

    fn form() -> Form<'static> {
        Form::new()
            .question("name", Input::new("Project name").default("my-app"))
            .question("target", Select::new("Deploy to").options(["staging", "production"]))
            .question("tag", Confirm::new("Tag the release?").default(true))
    }

    #[test]
    fn asks_each_then_looks_them_over() {
        let mut keys = typed("web");
        keys.extend([Key::Enter, Key::Down, Key::Enter, Key::Enter, Key::Enter]);
        let script = script(keys);
        let answers = form().ask().unwrap();
        assert_eq!(answers.text("name"), "web");
        assert_eq!(answers.choice("target"), 1);
        assert!(answers.yes("tag"));
        assert_eq!(
            script.answered(),
            [
                "●  Project name  web",
                "●  Deploy to  production",
                "●  Tag the release?  yes"
            ]
        );
        assert_eq!(script.remaining(), 0);
    }

    #[test]
    fn going_back_keeps_the_answer_to_change() {
        // answer the name, go back to it from the list, add to it, then carry on
        let mut keys = typed("web");
        keys.extend([Key::Enter, Key::Left]);
        keys.extend(typed("site"));
        keys.extend([Key::Enter, Key::Enter, Key::Char('n'), Key::Enter]);
        let script = script(keys);
        let answers = form().ask().unwrap();
        assert_eq!(answers.text("name"), "website");
        assert!(!answers.yes("tag"));
        assert_eq!(script.answered()[0], "●  Project name  website");
    }

    #[test]
    fn any_answer_can_be_changed_from_the_look_over() {
        let mut keys = vec![Key::Enter, Key::Enter, Key::Enter];
        // on the look over: down to "Deploy to", change it, then carry on
        keys.extend([Key::Down, Key::Down, Key::Enter, Key::Down, Key::Enter, Key::Enter]);
        let script = script(keys);
        let answers = form().ask().unwrap();
        assert_eq!(answers.choice("target"), 1);
        assert_eq!(answers.text("name"), "my-app");
        // in the order they were asked, whichever was changed last
        assert_eq!(
            script.answered(),
            [
                "●  Project name  my-app",
                "●  Deploy to  production",
                "●  Tag the release?  yes"
            ]
        );
    }

    #[test]
    fn back_from_the_look_over_asks_the_last_again() {
        let keys = [
            Key::Enter,
            Key::Enter,
            Key::Enter,
            Key::Left,
            Key::Char('n'),
            Key::Enter,
        ];
        let _script = script(keys);
        assert!(!form().ask().unwrap().yes("tag"));
    }

    #[test]
    fn cancelling_any_question_cancels_the_form() {
        let script = script([Key::Enter, Key::Esc]);
        assert!(matches!(form().ask(), Err(Error::Cancelled)));
        assert_eq!(
            script.answered(),
            ["●  Project name  my-app", "✕  Deploy to  cancelled"]
        );
    }

    #[test]
    fn without_a_terminal_each_question_answers_as_it_would_alone() {
        let form = Form::new().question("target", Select::new("Deploy to").options(["a", "b"]).selected(1));
        let answers = crate::output::with_terminal(false, || form.ask()).unwrap();
        assert_eq!(answers.choice("target"), 1);
        let form = Form::new().question("target", Select::new("Deploy to").options(["a", "b"]));
        assert!(matches!(
            crate::output::with_terminal(false, || form.ask()),
            Err(Error::NotATerminal)
        ));
    }

    #[test]
    fn a_list_with_nothing_to_choose_is_refused_up_front() {
        let _script = script([]);
        let form = Form::new().question("x", Select::new("Pick"));
        assert!(matches!(form.ask(), Err(Error::NoOptions)));
        assert!(Form::new().ask().is_ok());
    }

    #[test]
    fn lists_of_every_kind_start_from_their_last_answer() {
        let form = || {
            Form::new()
                .question("parts", MultiSelect::new("Parts").options(["a", "b", "c"]))
                .question("tag", Dropdown::new("Tag").options(["v1", "v2"]))
        };
        // tick a and c, go back from the tag, take the ticks as they are, then v2
        let keys = [
            Key::Char(' '),
            Key::Down,
            Key::Down,
            Key::Char(' '),
            Key::Enter,
            Key::Left,
            Key::Enter,
            Key::Down,
            Key::Enter,
            // change the tag from the look over: it starts on v2, so Enter keeps it
            Key::Down,
            Key::Down,
            Key::Enter,
            Key::Enter,
            Key::Enter,
        ];
        let script = script(keys);
        let answers = form().ask().unwrap();
        assert_eq!(answers.choices("parts"), [0, 2]);
        assert_eq!(answers.text("tag"), "v2");
        assert_eq!(script.answered(), ["●  Parts  a, c", "●  Tag  v2"]);
    }

    #[test]
    fn passwords_stay_hidden_in_the_look_over() {
        let mut keys = typed("hunter2");
        keys.extend([Key::Enter, Key::Esc]);
        let script = script(keys);
        let form = Form::new().question("token", Input::new("Token").password());
        // Esc on the look over cancels, leaving the answers and the cancelled look over
        assert!(matches!(form.ask(), Err(Error::Cancelled)));
        assert_eq!(script.answered(), ["●  Token  •••••••", "✕  All good?  cancelled"]);
    }

    #[test]
    fn questions_show_what_they_are() {
        let form = Form::new()
            .question("a", Confirm::new("Go?"))
            .question("b", Input::new("Name"))
            .question("c", Select::new("Pick").option("x"))
            .question("d", MultiSelect::new("Tick").option("x"))
            .question("e", Dropdown::new("Tag").option("x"));
        let shown = format!("{form:?}");
        for q in ["Confirm", "Input", "Select", "MultiSelect", "Dropdown"] {
            assert!(shown.contains(&format!("{q} {{")), "{shown}");
        }
        assert!(
            form.questions
                .iter()
                .map(|(_, q)| q.question())
                .eq(["Go?", "Name", "Pick", "Tick", "Tag"])
        );
        let custom = Form::new().question("t", Dropdown::new("Tag").allow_custom());
        assert!(custom.questions[0].1.answerable());
    }

    fn answered() -> Answers {
        let _script = script([Key::Enter, Key::Enter, Key::Enter, Key::Enter]);
        form().ask().unwrap()
    }

    #[test]
    #[should_panic(expected = "wasn't a yes or no question")]
    fn the_wrong_kind_of_answer_panics() {
        answered().yes("name");
    }

    #[test]
    #[should_panic(expected = "wasn't answered with text")]
    fn text_from_a_list_panics() {
        answered().text("target");
    }

    #[test]
    #[should_panic(expected = "wasn't a Select")]
    fn a_choice_from_text_panics() {
        answered().choice("name");
    }

    #[test]
    #[should_panic(expected = "wasn't a MultiSelect")]
    fn choices_from_one_choice_panics() {
        answered().choices("target");
    }

    #[test]
    #[should_panic(expected = "the form has no question called \"nope\"")]
    fn a_name_the_form_doesnt_have_panics() {
        answered().yes("nope");
    }
}
