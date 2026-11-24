//! A few questions asked as one form: ← goes back, and the answers are looked over (and any
//! changed) before going on.
//!
//!   cargo run --example form

use tinsel::prompt::{self, Confirm, Form, Input, MultiSelect, Select};

fn main() {
    let targets = ["staging", "production"];
    let services = ["api", "web", "worker"];
    let answers = Form::new()
        .question("name", Input::new("Project name").default("my-app"))
        .question("target", Select::new("Deploy to").options(targets))
        .question(
            "services",
            MultiSelect::new("Services").options(services).selected(&[0, 1]),
        )
        .question("tag", Confirm::new("Tag the release?").default(true))
        .ask();
    let answers = match answers {
        Ok(answers) => answers,
        Err(prompt::Error::Cancelled) => std::process::exit(130),
        Err(e) => {
            tinsel::Report::from_error("couldn't ask", &e).eprint();
            std::process::exit(1);
        }
    };

    let picked: Vec<&str> = answers.choices("services").iter().map(|&i| services[i]).collect();
    tinsel::success(&format!(
        "deploying {} ({}) to {}{}",
        answers.text("name"),
        picked.join(", "),
        targets[answers.choice("target")],
        if answers.yes("tag") { ", tagged" } else { "" }
    ));
}
