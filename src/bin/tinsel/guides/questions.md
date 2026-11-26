# Asking questions

Ask, and handle someone pressing Ctrl-C. Long lists filter as you type:

```rust
use tinsel::prompt;

match prompt::select("Deploy to", &["staging", "production"]) {
    Ok(i) => deploy(i),
    Err(prompt::Error::Cancelled) => std::process::exit(130),
    Err(e) => tinsel::Report::from_error("couldn't ask", &e).eprint(),
}
```

`tinsel prompts` asks every kind there is, to try them.

## Text, numbers and dates

Text questions can validate, and ask again until the answer passes:

```rust
let port = prompt::Input::new("Port")
    .placeholder("8080")
    .validate(|s| s.parse::<u16>().map(|_| ()).map_err(|_| "a port is 1-65535".into()))
    .ask()?;
let replicas: u32 = prompt::number("Replicas", Some(3))?;
let live_on = prompt::date("Go live on", None)?;                  // a calendar
let notes = prompt::editor("Release notes", "## What's new\n")?;   // opens $EDITOR
```

## Several at once

A `Form` asks several questions as one: ← goes back to change an answer, and at the end
they're all shown, to change any of them before going on:

```rust
use tinsel::prompt::{Confirm, Form, Input, Select};

let answers = Form::new()
    .question("name", Input::new("Project name").default("my-app"))
    .question("target", Select::new("Deploy to").options(["staging", "production"]))
    .question("tag", Confirm::new("Tag the release?").default(true))
    .ask()?;
```

## Grouped lists

Lists can be grouped, and say why an option can't be picked:

```rust
let service = prompt::Select::new("Service")
    .heading("Frontend").option("web").disabled("docs", "deploys paused")
    .heading("Backend").options(["api", "worker"])
    .ask()?;
```
