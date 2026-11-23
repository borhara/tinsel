//! Reading a program's arguments, described once for both parsing and `--help`.

use std::ffi::OsString;
use std::fmt;
use std::str::FromStr;

use crate::help::Help;
use crate::message::Report;

/// A flag or an option, from a spec like `-q, --quiet` or `-r, --region <NAME>`.
#[derive(Clone, Debug)]
struct Flag;

impl Flag {
    fn parse(_spec: &str, _about: &str) -> Flag {
        todo!()
    }

    /// How it's written in messages: the long name if it has one.
    fn shown(&self) -> String {
        todo!()
    }
}

/// A program's command line: its flags, options, arguments and subcommands, described once.
/// [`parse`](Cli::parse) reads the arguments the program was started with, and the same
/// description draws `--help` as a [`Help`].
#[derive(Clone, Debug, Default)]
pub struct Cli;

impl Cli {
    /// A program called `name`, which does `about`.
    pub fn new(_name: impl Into<String>, _about: impl Into<String>) -> Self {
        todo!()
    }

    /// The version, shown in help and printed by `-V` or `--version`.
    #[must_use]
    pub fn version(self, _version: impl Into<String>) -> Self {
        todo!()
    }

    /// A flag, `-q, --quiet`, or an option that takes a value, `--region <NAME>`.
    #[must_use]
    pub fn flag(self, _spec: impl AsRef<str>, _about: impl AsRef<str>) -> Self {
        todo!()
    }

    /// A positional argument: `<APP>` is needed, `[TAG]` isn't, and `<FILES>...` takes the rest.
    #[must_use]
    pub fn arg(self, _spec: impl AsRef<str>, _about: impl AsRef<str>) -> Self {
        todo!()
    }

    /// A subcommand, described the same way.
    #[must_use]
    pub fn command(self, _command: Cli) -> Self {
        todo!()
    }

    /// An example command line for the help, and what it does.
    #[must_use]
    pub fn example(self, _command: impl Into<String>, _about: impl Into<String>) -> Self {
        todo!()
    }

    /// A closing line for the help.
    #[must_use]
    pub fn footer(self, _text: impl Into<String>) -> Self {
        todo!()
    }

    /// The help this describes, with a usage line worked out from it.
    pub fn help(&self) -> Help {
        todo!()
    }

    /// Read the arguments the program was started with, exiting on `--help`, `--version` or
    /// a mistake.
    pub fn parse(&self) -> Args {
        todo!()
    }

    /// Read these arguments (without the program's name), saying what went wrong instead of
    /// exiting.
    ///
    /// # Errors
    ///
    /// A [`CliError`] for anything that couldn't be read, and for `--help` and `--version`.
    pub fn try_parse_from<I, S>(&self, _args: I) -> Result<Args, CliError>
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        todo!()
    }
}

/// The candidate closest to `typed`, if one is close enough to be a typo of it.
fn closest(_typed: &str, _candidates: impl Iterator<Item = String>) -> Option<String> {
    todo!()
}

/// How many characters must be added, removed, changed or swapped to turn `a` into `b`.
fn distance(_a: &str, _b: &str) -> usize {
    todo!()
}

/// The arguments a program was given, from [`Cli::parse`].
#[derive(Clone, Debug)]
pub struct Args;

impl Args {
    /// Whether a flag was given.
    pub fn flag(&self, _name: &str) -> bool {
        todo!()
    }

    /// How many times a flag was given.
    pub fn count(&self, _name: &str) -> usize {
        todo!()
    }

    /// An option's value, the last one given.
    pub fn value(&self, _name: &str) -> Option<&str> {
        todo!()
    }

    /// Every value given for an option, in order.
    pub fn values(&self, _name: &str) -> Vec<&str> {
        todo!()
    }

    /// An option's value parsed from text, exiting with a report when it doesn't parse.
    pub fn value_as<T: FromStr>(&self, _name: &str) -> Option<T> {
        todo!()
    }

    /// An option's value parsed from text.
    ///
    /// # Errors
    ///
    /// A [`CliError`] of kind [`CliErrorKind::BadValue`] when the value doesn't parse.
    pub fn try_value_as<T: FromStr>(&self, _name: &str) -> Result<Option<T>, CliError> {
        todo!()
    }

    /// A positional argument, by the name in its spec.
    pub fn arg(&self, _name: &str) -> Option<&str> {
        todo!()
    }

    /// Every word a positional argument took.
    pub fn args(&self, _name: &str) -> Vec<&str> {
        todo!()
    }

    /// The subcommand that was given, by name, with its own arguments.
    pub fn command(&self) -> Option<(&str, &Args)> {
        todo!()
    }
}

/// What [`Cli::try_parse_from`] ran into.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CliErrorKind {
    /// `-h` or `--help` was given.
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
#[derive(Clone, Debug)]
pub struct CliError {
    help: Box<Help>,
}

impl CliError {
    /// What happened.
    pub fn kind(&self) -> &CliErrorKind {
        todo!()
    }

    /// The mistake as a [`Report`].
    pub fn as_report(&self) -> Option<&Report> {
        todo!()
    }

    /// Print what this is and exit.
    pub fn exit(&self) -> ! {
        todo!()
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
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
