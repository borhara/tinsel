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

impl Question<'_> {
    fn question(&self) -> &str {
        todo!()
    }

    /// Whether there's anything to choose, for lists.
    fn answerable(&self) -> bool {
        todo!()
    }
}

/// Questions asked one after another as a group: ← goes back to the one before (with its
/// answer still there to change), and once they're all answered, a last question shows the
/// answers and offers to change any of them before going on.
#[derive(Debug, Default)]
pub struct Form<'a> {
    questions: Vec<(String, Question<'a>)>,
}

impl<'a> Form<'a> {
    /// A form with no questions yet.
    pub fn new() -> Self {
        Form::default()
    }

    /// Add a question, and the name its answer is found by in the [`Answers`].
    #[must_use]
    pub fn question(mut self, name: impl Into<String>, question: impl Into<Question<'a>>) -> Self {
        self.questions.push((name.into(), question.into()));
        self
    }

    /// Ask the questions, then offer to change any answer before returning them all.
    ///
    /// # Errors
    ///
    /// [`Error::Cancelled`] on Ctrl-C or Esc at any question, and otherwise whatever a question
    /// asked on its own would return.
    pub fn ask(self) -> Result<Answers> {
        todo!()
    }
}

/// The answers to a [`Form`], by the name each question was given.
#[derive(Clone, Debug)]
pub struct Answers;

impl Answers {
    /// A [`Confirm`]'s answer.
    pub fn yes(&self, _name: &str) -> bool {
        todo!()
    }

    /// An [`Input`]'s or a [`Dropdown`]'s answer.
    pub fn text(&self, _name: &str) -> &str {
        todo!()
    }

    /// A [`Select`]'s answer: the number of the option chosen.
    pub fn choice(&self, _name: &str) -> usize {
        todo!()
    }

    /// A [`MultiSelect`]'s answer: the numbers of the options ticked, in order.
    pub fn choices(&self, _name: &str) -> &[usize] {
        todo!()
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
