//! A form without a terminal answers each question as it would on its own: lines read from
//! stdin, or what's selected. Run in this test binary run again as a child, with stdin piped.

use std::io::Write;
use std::process::{Command, Stdio};

use tinsel::prompt::{Confirm, Dropdown, Form, Input, MultiSelect, Select};

/// The child's half: asks the form and prints what came back.
#[test]
#[ignore = "run by the test below, in a child process"]
fn form_child() {
    if std::env::var("TINSEL_FORM_CHILD").is_err() {
        return;
    }
    let answers = Form::new()
        .question("go", Confirm::new("Go?"))
        .question("name", Input::new("Name"))
        .question("env", Select::new("Env").options(["dev", "prod"]).selected(1))
        .question("parts", MultiSelect::new("Parts").options(["a", "b"]).selected(&[0]))
        .question("tag", Dropdown::new("Tag").options(["v1", "v2"]).selected(0))
        .ask()
        .unwrap();
    println!(
        "[{} {} {} {:?} {}]",
        answers.yes("go"),
        answers.text("name"),
        answers.choice("env"),
        answers.choices("parts"),
        answers.text("tag")
    );
}

#[test]
fn a_form_reads_piped_answers() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["form_child", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env("TINSEL_FORM_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"yes\nweb\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{printed}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(printed.contains("[true web 1 [0] v1]"), "{printed}");
}
