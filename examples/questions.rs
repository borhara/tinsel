//! Asking questions, and handling someone pressing Ctrl-C partway through.
//!
//!   cargo run --example questions

use tinsel::prelude::*;

fn main() {
    match ask() {
        Ok(()) => {}
        // Ctrl-C or Esc: the question already shows "cancelled", so just leave quietly
        Err(prompt::Error::Cancelled) => std::process::exit(130),
        Err(e) => {
            Report::from_error("couldn't set up the project", &e)
                .hint("run this in a terminal")
                .eprint();
            std::process::exit(1);
        }
    }
}

fn ask() -> prompt::Result<()> {
    println!();
    let name = prompt::input("Project name", Some("my-app"))?;
    let template = ["web", "cli", "library"];
    let kind = prompt::select("Template", &template)?;
    let features = ["tests", "ci", "docs", "docker"];
    let picked = prompt::multiselect("Include", &features, &[0, 1])?;
    if !prompt::confirm("Create it?", true)? {
        hint("nothing created");
        return Ok(());
    }
    println!();
    let chosen: Vec<&str> = picked.iter().map(|&i| features[i]).collect();
    success(&format!(
        "created {name} from the {} template with {}",
        template[kind],
        chosen.join(", ")
    ));
    Ok(())
}
