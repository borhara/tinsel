//! Reading a program's arguments, described once for both parsing and `--help`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::str::FromStr;

use crate::help::Help;
use crate::message::Report;

/// A flag or an option, from a spec like `-q, --quiet` or `-r, --region <NAME>`.
#[derive(Clone, Debug)]
struct Flag {
    spec: String,
    about: String,
    short: Option<char>,
    long: Option<String>,
    /// What its value is called, for options; flags have none.
    value: Option<String>,
}

impl Flag {
    fn parse(spec: &str, about: &str) -> Flag {
        let mut flag = Flag {
            spec: spec.into(),
            about: about.into(),
            short: None,
            long: None,
            value: None,
        };
        for word in spec.split([' ', ',']).filter(|w| !w.is_empty()) {
            if let Some(long) = word.strip_prefix("--") {
                // `--name=VALUE` names its value the way `--name <VALUE>` does
                match long.split_once('=') {
                    Some((name, value)) => {
                        flag.long = Some(name.into());
                        flag.value = Some(value.trim_matches(['<', '>', '[', ']']).into());
                    }
                    None => flag.long = Some(long.into()),
                }
            } else if let Some(short) = word.strip_prefix('-') {
                flag.short = short.chars().next();
            } else if word.starts_with(['<', '[']) {
                flag.value = Some(word.trim_matches(['<', '>', '[', ']', '.']).into());
            }
        }
        flag
    }

    /// Whether `name` (`quiet`, `--quiet`, `q` or `-q`) is this flag.
    fn is(&self, name: &str) -> bool {
        let name = name.trim_start_matches('-');
        self.long.as_deref() == Some(name) || (name.chars().count() == 1 && self.short == name.chars().next())
    }

    /// How it's written in messages: the long name if it has one.
    fn shown(&self) -> String {
        match &self.long {
            Some(long) => format!("--{long}"),
            None => format!("-{}", self.short.unwrap_or('?')),
        }
    }
}

/// A positional argument, from a spec like `<APP>`, `[TAG]` or `<FILES>...`.
#[derive(Clone, Debug)]
struct Arg {
    spec: String,
    about: String,
    name: String,
    required: bool,
    many: bool,
}

impl Arg {
    fn parse(spec: &str, about: &str) -> Arg {
        Arg {
            spec: spec.into(),
            about: about.into(),
            name: spec.trim_matches(['<', '>', '[', ']', '.']).to_lowercase(),
            required: spec.starts_with('<'),
            many: spec.ends_with("..."),
        }
    }
}

/// A program's command line: its flags, options, arguments and subcommands, described once.
/// [`parse`](Cli::parse) reads the arguments the program was started with, and the same
/// description draws `--help` as a [`Help`].
///
/// Flags are written as they'd appear in help: `-q, --quiet` is a flag, and anything with a
/// value after it, `-r, --region <NAME>`, is an option that takes one. Arguments are `<APP>`
/// when they're needed, `[TAG]` when they're not, and `<FILES>...` for any number of them
/// (only the last can be). `-h, --help` comes free, and so does `-V, --version` once there's
/// a [`version`](Cli::version).
///
/// ```
/// use tinsel::Cli;
///
/// let cli = Cli::new("deploy", "ship an app to the cloud")
///     .flag("-r, --region <NAME>", "where to deploy")
///     .flag("-q, --quiet", "only print problems")
///     .arg("<APP>", "what to ship");
///
/// let args = cli.try_parse_from(["my-app", "--region", "us-east-1"])?;
/// assert_eq!(args.arg("app"), Some("my-app"));
/// assert_eq!(args.value("region"), Some("us-east-1"));
/// assert!(!args.flag("quiet"));
/// # Ok::<(), tinsel::CliError>(())
/// ```
///
/// A mistake is drawn as a [`Report`], with the flag or command that was probably meant:
///
/// ```text
///   ✕  there's no --regon flag
///   →  did you mean --region?
/// ```
///
/// This covers what most programs need and stops there: there are no rules between flags,
/// no defaults from the environment, and subcommands go one level deep. A flag the
/// subcommand doesn't have but the program does is the program's, so `deploy status -q` works.
#[derive(Clone, Debug, Default)]
pub struct Cli {
    name: String,
    about: String,
    version: Option<String>,
    flags: Vec<Flag>,
    args: Vec<Arg>,
    commands: Vec<Cli>,
    examples: Vec<(String, String)>,
    footer: Option<String>,
}

impl Cli {
    /// A program called `name`, which does `about`. Also how a subcommand starts, for
    /// [`command`](Cli::command).
    pub fn new(name: impl Into<String>, about: impl Into<String>) -> Self {
        Cli {
            name: name.into(),
            about: about.into(),
            ..Self::default()
        }
    }

    /// The version, shown in help and printed by `-V` or `--version`.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// A flag, `-q, --quiet`, or an option that takes a value, `--region <NAME>`. Either
    /// name can be left out.
    #[must_use]
    pub fn flag(mut self, spec: impl AsRef<str>, about: impl AsRef<str>) -> Self {
        self.flags.push(Flag::parse(spec.as_ref(), about.as_ref()));
        self
    }

    /// A positional argument: `<APP>` is needed, `[TAG]` isn't, and `<FILES>...` takes the
    /// rest. They're filled in the order they're added.
    #[must_use]
    pub fn arg(mut self, spec: impl AsRef<str>, about: impl AsRef<str>) -> Self {
        self.args.push(Arg::parse(spec.as_ref(), about.as_ref()));
        self
    }

    /// A subcommand, described the same way.
    ///
    /// ```
    /// use tinsel::Cli;
    ///
    /// let cli = Cli::new("deploy", "ship an app")
    ///     .flag("-q, --quiet", "only print problems")
    ///     .command(Cli::new("logs", "recent logs").flag("--since <TIME>", "how far back"));
    ///
    /// let args = cli.try_parse_from(["logs", "--since", "5m", "-q"])?;
    /// let (name, logs) = args.command().unwrap();
    /// assert_eq!((name, logs.value("since")), ("logs", Some("5m")));
    /// assert!(args.flag("quiet"));
    /// # Ok::<(), tinsel::CliError>(())
    /// ```
    #[must_use]
    pub fn command(mut self, command: Cli) -> Self {
        self.commands.push(command);
        self
    }

    /// An example command line for the help, and what it does.
    #[must_use]
    pub fn example(mut self, command: impl Into<String>, about: impl Into<String>) -> Self {
        self.examples.push((command.into(), about.into()));
        self
    }

    /// A closing line for the help: where the docs are, how to report bugs.
    #[must_use]
    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.footer = Some(text.into());
        self
    }

    /// The help this describes, with a usage line worked out from it.
    pub fn help(&self) -> Help {
        self.help_as(&self.name)
    }

    fn help_as(&self, path: &str) -> Help {
        let mut help = Help::new(path, &self.about);
        if let Some(v) = &self.version {
            help = help.version(v);
        }
        let mut usage = path.to_string();
        if !self.flags.is_empty() {
            usage += " [OPTIONS]";
        }
        if self.args.is_empty() && !self.commands.is_empty() {
            usage += " <COMMAND>";
        }
        for arg in &self.args {
            usage = format!("{usage} {}", arg.spec);
        }
        help = help.usage(usage);
        // with arguments of its own, a command is another way to call it
        if !self.args.is_empty() && !self.commands.is_empty() {
            help = help.usage(format!("{path} <COMMAND>"));
        }
        if !self.args.is_empty() {
            help = help.group("arguments");
            for arg in &self.args {
                help = help.item(&arg.spec, &arg.about);
            }
        }
        for command in &self.commands {
            help = help.command(&command.name, &command.about);
        }
        for flag in &self.flags {
            help = help.flag(&flag.spec, &flag.about);
        }
        help = help.flag(self.builtin_spec("-h", "--help"), "show this help");
        if self.version.is_some() {
            help = help.flag(self.builtin_spec("-V", "--version"), "show the version");
        }
        for (command, about) in &self.examples {
            help = help.example(command, about);
        }
        if let Some(footer) = &self.footer {
            help = help.footer(footer);
        }
        help
    }

    /// `-h, --help`, or just `--help` when `-h` is taken.
    fn builtin_spec(&self, short: &str, long: &str) -> String {
        if self.find(short).is_some() {
            long.into()
        } else {
            format!("{short}, {long}")
        }
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.flags.iter().position(|f| f.is(name))
    }

    /// Read the arguments the program was started with. On `--help` or `--version` that's
    /// printed and the program exits; on a mistake a [`Report`] says what's wrong on stderr
    /// and the program exits with status 2.
    pub fn parse(&self) -> Args {
        let args = std::env::args_os().skip(1);
        self.try_parse_from(args).unwrap_or_else(|e| e.exit())
    }

    /// Read these arguments (without the program's name), saying what went wrong instead of
    /// exiting. `--help` and `--version` come back as errors too, for [`CliError::exit`] to
    /// print.
    ///
    /// # Errors
    ///
    /// A [`CliError`] for a flag, command or argument that isn't known, an option without
    /// its value, a needed argument left out, or `--help` and `--version`.
    pub fn try_parse_from<I, S>(&self, args: I) -> Result<Args, CliError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let args: Vec<String> = args
            .into_iter()
            .map(|a| a.into().to_string_lossy().into_owned())
            .collect();
        let mut parsed = Args::new(self);
        self.parse_into(&args, &mut parsed, None, &self.name)?;
        Ok(parsed)
    }

    /// Parse `args` into `out`, handing flags this command doesn't know to its `parent`.
    fn parse_into(
        &self,
        args: &[String],
        out: &mut Args,
        mut parent: Option<(&Cli, &mut Args)>,
        path: &str,
    ) -> Result<(), CliError> {
        let mut positional = Vec::new();
        let mut rest = args.iter();
        let mut only_args = false;
        while let Some(word) = rest.next() {
            if only_args || word == "-" || !word.starts_with('-') {
                if positional.is_empty() && !self.commands.is_empty() {
                    // a word that isn't a command is the program's own argument, if it takes one
                    let command = self.commands.iter().find(|c| &c.name == word);
                    let Some(command) = command else {
                        if self.args.is_empty() {
                            return Err(self.unknown_command(word, path));
                        }
                        positional.push(word.clone());
                        continue;
                    };
                    let rest: Vec<String> = rest.cloned().collect();
                    let mut sub = Args::new(command);
                    command.parse_into(&rest, &mut sub, Some((self, &mut *out)), &format!("{path} {word}"))?;
                    out.command = Some((command.name.clone(), Box::new(sub)));
                    return self.check_args(positional, out, path);
                }
                positional.push(word.clone());
                continue;
            }
            if word == "--" {
                only_args = true;
                continue;
            }
            // `--name`, `--name=value`, or a run of short flags like `-qv` or `-rVALUE`
            let names: Vec<(String, Option<String>)> = match word.strip_prefix("--") {
                Some(long) => match long.split_once('=') {
                    Some((name, value)) => vec![(format!("--{name}"), Some(value.into()))],
                    None => vec![(word.clone(), None)],
                },
                None => {
                    let chars: Vec<char> = word[1..].chars().collect();
                    let mut names = Vec::new();
                    for (i, c) in chars.iter().enumerate() {
                        let name = format!("-{c}");
                        let takes = self.find(&name).is_some_and(|f| self.flags[f].value.is_some());
                        let after: String = chars[i + 1..].iter().collect();
                        if takes && !after.is_empty() {
                            names.push((name, Some(after)));
                            break;
                        }
                        names.push((name, None));
                    }
                    names
                }
            };
            for (name, inline) in names {
                if self.builtin(&name, "-h", "--help") {
                    return Err(CliError::new(CliErrorKind::Help, self.help_as(path)));
                }
                // a subcommand without a version of its own has the program's
                let version = self
                    .version
                    .as_deref()
                    .or(parent.as_ref().and_then(|(p, _)| p.version.as_deref()));
                if let Some(version) = version.filter(|_| self.builtin(&name, "-V", "--version")) {
                    let program = path.split(' ').next().unwrap_or(path);
                    let line = format!("{program} {version}");
                    return Err(CliError::new(CliErrorKind::Version(line), self.help_as(path)));
                }
                let (cli, into): (&Cli, &mut Args) = match (self.find(&name), parent.as_mut()) {
                    (Some(_), _) => (self, &mut *out),
                    (None, Some((p, a))) if p.find(&name).is_some() => (*p, &mut **a),
                    _ => return Err(self.unknown_flag(&name, path)),
                };
                let flag = &cli.flags[cli.find(&name).unwrap_or_default()];
                let key = flag.shown();
                match (&flag.value, inline) {
                    (None, Some(_)) => {
                        let report = Report::new(format!("{key} doesn't take a value"))
                            .hint(format!("give it as {key} on its own"));
                        return Err(
                            CliError::new(CliErrorKind::UnexpectedValue(key), self.help_as(path)).report(report)
                        );
                    }
                    (None, None) => *into.counts.entry(key).or_default() += 1,
                    (Some(_), Some(v)) => into.values.entry(key).or_default().push(v),
                    (Some(value), None) => match rest.next() {
                        Some(v) => into.values.entry(key).or_default().push(v.clone()),
                        None => {
                            let report =
                                Report::new(format!("{key} needs a value")).hint(format!("give it as {key} <{value}>"));
                            return Err(
                                CliError::new(CliErrorKind::MissingValue(key), self.help_as(path)).report(report)
                            );
                        }
                    },
                }
            }
        }
        self.check_args(positional, out, path)
    }

    /// Whether `name` is the built-in `short` or `long`, and not one of this command's own.
    fn builtin(&self, name: &str, short: &str, long: &str) -> bool {
        self.find(name).is_none() && (name == short || name == long)
    }

    /// Hand out the positional arguments, and complain about any missing or left over.
    fn check_args(&self, positional: Vec<String>, out: &mut Args, path: &str) -> Result<(), CliError> {
        let mut words = positional.into_iter();
        for arg in &self.args {
            let taken: Vec<String> = if arg.many {
                words.by_ref().collect()
            } else {
                words.next().into_iter().collect()
            };
            if taken.is_empty() && arg.required {
                let report =
                    Report::new(format!("{} needs {}", path, arg.spec)).hint(format!("{} is {}", arg.spec, arg.about));
                return Err(
                    CliError::new(CliErrorKind::MissingArg(arg.name.clone()), self.help_as(path)).report(report),
                );
            }
            out.positional.insert(arg.name.clone(), taken);
        }
        if let Some(extra) = words.next() {
            let hint = format!("run {path} --help to see what it takes");
            let report = Report::new(format!("{path} doesn't take “{extra}”")).hint(hint);
            return Err(CliError::new(CliErrorKind::UnexpectedArg(extra), self.help_as(path)).report(report));
        }
        Ok(())
    }

    fn unknown_flag(&self, name: &str, path: &str) -> CliError {
        let known = self
            .flags
            .iter()
            .filter_map(|f| f.long.as_ref().map(|l| format!("--{l}")));
        let hint = match closest(name, known) {
            Some(near) => format!("did you mean {near}?"),
            None => format!("run {path} --help to see the options"),
        };
        let report = Report::new(format!("there's no {name} flag")).hint(hint);
        CliError::new(CliErrorKind::UnknownFlag(name.into()), self.help_as(path)).report(report)
    }

    fn unknown_command(&self, word: &str, path: &str) -> CliError {
        let hint = match closest(word, self.commands.iter().map(|c| c.name.clone())) {
            Some(near) => format!("did you mean {path} {near}?"),
            None => format!("run {path} --help to see the commands"),
        };
        let report = Report::new(format!("there's no {path} {word} command")).hint(hint);
        CliError::new(CliErrorKind::UnknownCommand(word.into()), self.help_as(path)).report(report)
    }
}

/// The candidate closest to `typed`, if one is close enough to be a typo of it.
fn closest(typed: &str, candidates: impl Iterator<Item = String>) -> Option<String> {
    let allowed = (typed.chars().count() / 3).max(1);
    candidates
        .map(|c| (distance(typed, &c), c))
        .filter(|(d, _)| *d <= allowed)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// How many characters must be added, removed, changed or swapped to turn `a` into `b`.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let change = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + change);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// The arguments a program was given, from [`Cli::parse`]. Flags, options and arguments are
/// looked up by name: a flag or option by its long or short name (`quiet`, `--quiet` or
/// `q`), an argument by the name in its spec (`app` for `<APP>`).
///
/// Asking for a name the [`Cli`] doesn't describe panics, so a typo shows up the first time
/// the code runs rather than as a flag that never seems to be set.
#[derive(Clone, Debug)]
pub struct Args {
    flags: Vec<Flag>,
    arg_names: Vec<String>,
    counts: HashMap<String, usize>,
    values: HashMap<String, Vec<String>>,
    positional: HashMap<String, Vec<String>>,
    command: Option<(String, Box<Args>)>,
}

impl Args {
    fn new(cli: &Cli) -> Args {
        Args {
            flags: cli.flags.clone(),
            arg_names: cli.args.iter().map(|a| a.name.clone()).collect(),
            counts: HashMap::new(),
            values: HashMap::new(),
            positional: HashMap::new(),
            command: None,
        }
    }

    fn key(&self, name: &str) -> String {
        match self.flags.iter().find(|f| f.is(name)) {
            Some(f) => f.shown(),
            None => panic!("there's no flag called {name:?}: add it with Cli::flag"),
        }
    }

    /// Whether a flag was given.
    pub fn flag(&self, name: &str) -> bool {
        self.count(name) > 0
    }

    /// How many times a flag was given, for `-v`, `-vv`, `-vvv`.
    pub fn count(&self, name: &str) -> usize {
        self.counts.get(&self.key(name)).copied().unwrap_or(0)
    }

    /// An option's value, the last one given if it was given more than once.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values(name).last().copied()
    }

    /// Every value given for an option, in order, for options that can be given again.
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.values
            .get(&self.key(name))
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// An option's value as a number, a path or anything else that parses from text. A
    /// value that doesn't parse is drawn as a [`Report`] on stderr, and the program exits
    /// with status 2, as for any other mistake on the command line.
    ///
    /// ```
    /// let cli = tinsel::Cli::new("serve", "serve files").flag("-p, --port <PORT>", "where to listen");
    /// let args = cli.try_parse_from(["--port", "8080"])?;
    /// assert_eq!(args.value_as::<u16>("port"), Some(8080));
    /// # Ok::<(), tinsel::CliError>(())
    /// ```
    pub fn value_as<T: FromStr>(&self, name: &str) -> Option<T> {
        self.try_value_as(name).unwrap_or_else(|e| e.exit())
    }

    /// An option's value parsed as [`value_as`](Args::value_as) does, with a value that
    /// doesn't parse as an error rather than an exit.
    ///
    /// # Errors
    ///
    /// A [`CliError`] of kind [`CliErrorKind::BadValue`] when the value doesn't parse.
    pub fn try_value_as<T: FromStr>(&self, name: &str) -> Result<Option<T>, CliError> {
        let Some(value) = self.value(name) else {
            return Ok(None);
        };
        value.parse().map(Some).map_err(|_| {
            let key = self.key(name);
            let what = crate::prompt::kind_of_number(std::any::type_name::<T>());
            let report = Report::new(format!("{key} can't be “{value}”"));
            let report = if is_number(std::any::type_name::<T>()) {
                report.hint(format!("give it a {what}"))
            } else {
                report
            };
            CliError::new(CliErrorKind::BadValue(key), Help::default()).report(report)
        })
    }

    /// A positional argument, by the name in its spec: `app` for `<APP>`. For one that
    /// takes the rest (`<FILES>...`), the first of them.
    pub fn arg(&self, name: &str) -> Option<&str> {
        self.args(name).first().copied()
    }

    /// Every word a positional argument took: all of them for `<FILES>...`, one or none
    /// for the others.
    pub fn args(&self, name: &str) -> Vec<&str> {
        let name = name.to_lowercase();
        assert!(
            self.arg_names.contains(&name),
            "there's no argument called {name:?}: add it with Cli::arg"
        );
        self.positional
            .get(&name)
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// The subcommand that was given, by name, with its own arguments.
    pub fn command(&self) -> Option<(&str, &Args)> {
        self.command.as_ref().map(|(name, args)| (name.as_str(), &**args))
    }
}

/// Whether a type's name is one of the number types.
fn is_number(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

/// What [`Cli::try_parse_from`] ran into.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CliErrorKind {
    /// `-h` or `--help` was given: the help is what to print.
    Help,
    /// `-V` or `--version` was given, with the line to print.
    Version(String),
    /// A flag no one described, as typed.
    UnknownFlag(String),
    /// A subcommand that doesn't exist, as typed.
    UnknownCommand(String),
    /// An option given without its value.
    MissingValue(String),
    /// A flag given a value with `=`, though it doesn't take one.
    UnexpectedValue(String),
    /// A needed argument that was left out, by name.
    MissingArg(String),
    /// A word left over once every argument had one.
    UnexpectedArg(String),
    /// An option's value that didn't parse as what was asked for.
    BadValue(String),
}

/// Why the arguments couldn't be read, or that someone asked for `--help` or `--version`.
/// [`exit`](CliError::exit) prints whichever it is and exits the way programs do.
#[derive(Clone, Debug)]
pub struct CliError {
    kind: CliErrorKind,
    help: Box<Help>,
    report: Option<Box<Report>>,
}

impl CliError {
    fn new(kind: CliErrorKind, help: Help) -> CliError {
        CliError {
            kind,
            help: Box::new(help),
            report: None,
        }
    }

    fn report(mut self, report: Report) -> CliError {
        self.report = Some(Box::new(report));
        self
    }

    /// What happened.
    pub fn kind(&self) -> &CliErrorKind {
        &self.kind
    }

    /// The mistake as a [`Report`], for printing it your own way. `None` for `--help` and
    /// `--version`, which aren't mistakes.
    pub fn as_report(&self) -> Option<&Report> {
        self.report.as_deref()
    }

    /// Print what this is and exit: the help (with its entrance on a terminal) or the
    /// version on stdout with status 0, or a mistake on stderr with status 2.
    pub fn exit(&self) -> ! {
        match &self.kind {
            CliErrorKind::Help => self.help.animate(),
            CliErrorKind::Version(line) => println!("{line}"),
            _ => {
                self.report.iter().for_each(|r| r.eprint());
                std::process::exit(2);
            }
        }
        std::process::exit(0)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            CliErrorKind::Help => f.write_str("help was asked for"),
            CliErrorKind::Version(line) => f.write_str(line),
            // every mistake comes with a report
            _ => f.write_str(self.report.as_ref().map_or("", |r| r.title())),
        }
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli() -> Cli {
        Cli::new("deploy", "ship an app")
            .version("1.0")
            .flag("-r, --region <NAME>", "where to deploy")
            .flag("-q, --quiet", "only print problems")
            .flag("-v, --verbose", "say more")
            .arg("<APP>", "what to ship")
            .arg("[TAGS]...", "labels for the release")
    }

    fn kind(args: &[&str]) -> CliErrorKind {
        cli().try_parse_from(args.iter().copied()).unwrap_err().kind().clone()
    }

    fn hint(cli: &Cli, args: &[&str]) -> String {
        crate::set_color_mode(crate::ColorMode::None);
        cli.try_parse_from(args.iter().copied())
            .unwrap_err()
            .as_report()
            .unwrap()
            .lines()
            .plain()
    }

    #[test]
    fn reads_flags_options_and_arguments() {
        let args = cli()
            .try_parse_from(["my-app", "-vv", "--region=eu", "a", "-q", "b"])
            .unwrap();
        assert_eq!(args.arg("app"), Some("my-app"));
        assert_eq!(args.args("tags"), ["a", "b"]);
        assert_eq!(args.value("region"), Some("eu"));
        assert_eq!(args.count("v"), 2);
        assert!(args.flag("--quiet"));
    }

    #[test]
    fn short_options_take_the_rest_of_their_word_or_the_next() {
        let args = cli().try_parse_from(["app", "-qreu"]).unwrap();
        assert_eq!(args.value("r"), Some("eu"));
        assert!(args.flag("q"));
        let args = cli().try_parse_from(["app", "-r", "-5", "-r", "us"]).unwrap();
        assert_eq!(args.values("region"), ["-5", "us"]);
        assert_eq!(args.value("region"), Some("us"));
    }

    #[test]
    fn a_double_dash_ends_the_flags() {
        let args = cli().try_parse_from(["--", "-q", "-"]).unwrap();
        assert_eq!(args.arg("app"), Some("-q"));
        assert_eq!(args.args("tags"), ["-"]);
        assert!(!args.flag("quiet"));
    }

    #[test]
    fn says_what_went_wrong() {
        assert_eq!(kind(&["app", "--regon"]), CliErrorKind::UnknownFlag("--regon".into()));
        assert_eq!(
            kind(&["app", "--region"]),
            CliErrorKind::MissingValue("--region".into())
        );
        assert_eq!(
            kind(&["app", "--quiet=yes"]),
            CliErrorKind::UnexpectedValue("--quiet".into())
        );
        assert_eq!(kind(&[]), CliErrorKind::MissingArg("app".into()));
        assert_eq!(kind(&["-h"]), CliErrorKind::Help);
        assert_eq!(kind(&["app", "--version"]), CliErrorKind::Version("deploy 1.0".into()));
        let one = Cli::new("x", "y").arg("<A>", "a");
        let err = one.try_parse_from(["a", "b"]).unwrap_err();
        assert_eq!(err.kind(), &CliErrorKind::UnexpectedArg("b".into()));
    }

    #[test]
    fn suggests_what_was_probably_meant() {
        assert!(hint(&cli(), &["app", "--regon"]).contains("did you mean --region?"));
        assert!(hint(&cli(), &["app", "--reigon"]).contains("did you mean --region?"));
        assert!(hint(&cli(), &["app", "--colour"]).contains("run deploy --help to see the options"));
        let cli = Cli::new("deploy", "x").command(Cli::new("status", "y"));
        let report = hint(&cli, &["stauts"]);
        assert!(report.contains("there's no deploy stauts command"), "{report}");
        assert!(report.contains("did you mean deploy status?"), "{report}");
    }

    #[test]
    fn subcommands_have_their_own_flags_and_borrow_the_programs() {
        let cli = Cli::new("deploy", "x").flag("-q, --quiet", "quiet").command(
            Cli::new("logs", "y")
                .flag("--since <TIME>", "since")
                .arg("[APP]", "app"),
        );
        let args = cli.try_parse_from(["-q", "logs", "web", "--since", "5m"]).unwrap();
        let (name, logs) = args.command().unwrap();
        assert_eq!(
            (name, logs.arg("app"), logs.value("since")),
            ("logs", Some("web"), Some("5m"))
        );
        assert!(args.flag("quiet"));
        let err = cli.try_parse_from(["logs", "--help"]).unwrap_err();
        assert!(err.help.lines().plain().contains("usage  deploy logs [OPTIONS] [APP]"));
        assert!(cli.try_parse_from(Vec::<String>::new()).unwrap().command().is_none());
    }

    #[test]
    fn a_program_with_arguments_and_commands_takes_either() {
        let cli = Cli::new("deploy", "x")
            .version("2.0")
            .arg("[APP]", "app")
            .command(Cli::new("status", "y"));
        let args = cli.try_parse_from(["my-app"]).unwrap();
        assert_eq!((args.arg("app"), args.command().is_none()), (Some("my-app"), true));
        assert_eq!(cli.try_parse_from(["status"]).unwrap().command().unwrap().0, "status");
        let err = cli.try_parse_from(["status", "-V"]).unwrap_err();
        assert_eq!(err.kind(), &CliErrorKind::Version("deploy 2.0".into()));
        crate::set_color_mode(crate::ColorMode::None);
        let help = cli.help().lines().plain();
        assert!(
            help.contains("usage  deploy [APP]\n         deploy <COMMAND>"),
            "{help}"
        );
    }

    #[test]
    fn help_comes_from_the_same_description() {
        crate::set_color_mode(crate::ColorMode::None);
        let help = cli().help().lines().plain();
        assert!(help.contains("usage  deploy [OPTIONS] <APP> [TAGS]..."), "{help}");
        assert!(help.contains("  arguments\n    <APP>"), "{help}");
        assert!(help.contains("-r, --region <NAME>"), "{help}");
        assert!(help.contains("-h, --help"), "{help}");
        assert!(help.contains("-V, --version"), "{help}");
        // -h taken by the program itself leaves --help on its own
        let own = Cli::new("x", "y")
            .flag("-h, --host <HOST>", "where")
            .help()
            .lines()
            .plain();
        assert!(own.contains("    --help ") && !own.contains("-V"), "{own}");
    }

    #[test]
    fn parses_values_into_types() {
        let cli = Cli::new("serve", "x").flag("--port <PORT>", "port");
        let args = cli.try_parse_from(["--port", "eighty"]).unwrap();
        let err = args.try_value_as::<u16>("port").unwrap_err();
        assert_eq!(err.kind(), &CliErrorKind::BadValue("--port".into()));
        crate::set_color_mode(crate::ColorMode::None);
        let report = err.as_report().unwrap().lines().plain();
        assert!(report.contains("--port can't be “eighty”"), "{report}");
        assert!(report.contains("give it a whole number of 0 or more"), "{report}");
        assert_eq!(
            cli.try_parse_from(["--port", "80"]).unwrap().value_as::<u16>("port"),
            Some(80)
        );
    }

    #[test]
    fn flags_can_be_written_every_way_help_writes_them() {
        let cli = Cli::new("x", "y")
            .flag("--level=<N>", "how much")
            .flag("-n <COUNT>", "how many")
            .flag("-f", "force");
        let args = cli.try_parse_from(["--level", "3", "-n2", "-f"]).unwrap();
        assert_eq!(
            (args.value("level"), args.value("n"), args.flag("f")),
            (Some("3"), Some("2"), true)
        );
        let err = cli.try_parse_from(["-n"]).unwrap_err();
        assert_eq!(err.kind(), &CliErrorKind::MissingValue("-n".into()));
        assert_eq!(Flag::parse("<ODD>", "no names").shown(), "-?");
    }

    #[test]
    fn a_subcommand_borrows_flags_given_after_it() {
        let cli = Cli::new("deploy", "x")
            .flag("-q, --quiet", "quiet")
            .command(Cli::new("logs", "y"));
        let args = cli.try_parse_from(["logs", "-q"]).unwrap();
        assert!(args.flag("quiet") && args.command().is_some());
        // nothing close enough to suggest
        let report = hint(&cli, &["zzzzzz"]);
        assert!(report.contains("run deploy --help to see the commands"), "{report}");
    }

    #[test]
    fn examples_and_a_footer_go_in_the_help() {
        crate::set_color_mode(crate::ColorMode::None);
        let help = Cli::new("x", "y")
            .example("x run", "runs it")
            .footer("docs.rs/x")
            .help()
            .lines()
            .plain();
        assert!(
            help.contains("$ x run") && help.contains("runs it") && help.contains("docs.rs/x"),
            "{help}"
        );
    }

    #[test]
    fn values_left_out_or_of_other_types() {
        let cli = Cli::new("x", "y").flag("--addr <IP>", "where");
        let none = cli.try_parse_from(Vec::<String>::new()).unwrap();
        assert_eq!(none.try_value_as::<u16>("addr").unwrap(), None);
        let args = cli.try_parse_from(["--addr", "here"]).unwrap();
        let err = args.try_value_as::<std::net::IpAddr>("addr").unwrap_err();
        crate::set_color_mode(crate::ColorMode::None);
        // no hint for what isn't a number: the type's name wouldn't mean anything to someone
        let report = err.as_report().unwrap().lines().plain();
        assert!(
            report.contains("✕  --addr can't be “here”") && !report.contains('→'),
            "{report}"
        );
    }

    #[test]
    fn errors_say_what_they_are() {
        let cli = cli();
        let shown = |args: &[&str]| cli.try_parse_from(args.iter().copied()).unwrap_err().to_string();
        assert_eq!(shown(&["--help"]), "help was asked for");
        assert_eq!(shown(&["-V"]), "deploy 1.0");
        assert_eq!(shown(&["app", "--regon"]), "there's no --regon flag");
        assert!(cli.try_parse_from(["-h"]).unwrap_err().as_report().is_none());
    }

    #[test]
    #[should_panic(expected = "there's no argument called \"nope\"")]
    fn asking_for_an_argument_no_one_described_panics() {
        cli().try_parse_from(["app"]).unwrap().arg("nope");
    }

    #[test]
    #[should_panic(expected = "there's no flag called \"colour\"")]
    fn asking_for_a_flag_no_one_described_panics() {
        cli().try_parse_from(["app"]).unwrap().flag("colour");
    }

    #[test]
    fn typos_are_a_short_distance_apart() {
        assert_eq!(distance("region", "region"), 0);
        assert_eq!(distance("regon", "region"), 1);
        assert_eq!(distance("reigon", "region"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(
            closest("--stauts", ["--status".to_string(), "--start".to_string()].into_iter()).as_deref(),
            Some("--status")
        );
    }
}
