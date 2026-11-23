//! Reading the command line: flags, arguments and subcommands, with `--help` drawn from the
//! same description. Try `-- --help`, `-- ship my-app --regon eu` or `-- stauts`.

use tinsel::Cli;

fn main() {
    let cli = Cli::new("deploy", "ship an app to the cloud")
        .version(env!("CARGO_PKG_VERSION"))
        .flag("-q, --quiet", "only print problems")
        .command(
            Cli::new("ship", "deploy an app")
                .flag("-r, --region <NAME>", "where to deploy (default: eu-west-2)")
                .flag("--replicas <N>", "how many copies to run (default: 1)")
                .arg("<APP>", "what to ship"),
        )
        .command(Cli::new("status", "what's running where"))
        .command(Cli::new("logs", "recent logs").flag("--since <TIME>", "how far back (default: 5m)"))
        .example("deploy ship my-app --region us-east-1", "ship to the US east region");
    let args = cli.parse();

    match args.command() {
        Some(("ship", ship)) => {
            let app = ship.arg("app").unwrap_or_default();
            let region = ship.value("region").unwrap_or("eu-west-2");
            let replicas: u32 = ship.value_as("replicas").unwrap_or(1);
            if !args.flag("quiet") {
                tinsel::success(&format!("shipped {app} to {region} ×{replicas}"));
            }
        }
        Some(("status", _)) => tinsel::info("everything's running"),
        Some(("logs", logs)) => tinsel::info(&format!("logs since {}", logs.value("since").unwrap_or("5m"))),
        _ => cli.help().print(),
    }
}
