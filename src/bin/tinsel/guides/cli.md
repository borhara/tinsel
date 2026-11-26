# Command lines and help

## Read the command line

From one description, which draws `--help` too. A mistake is a report that says what was
probably meant:

```rust
let cli = tinsel::Cli::new("deploy", "ship an app to the cloud")
    .version(env!("CARGO_PKG_VERSION"))
    .flag("-r, --region <NAME>", "where to deploy")
    .flag("-q, --quiet", "only print problems")
    .flag("-n, --replicas <N>", "how many to run")
    .arg("<APP>", "what to ship")
    .command(tinsel::Cli::new("status", "what's running where"));
let args = cli.parse();               // --help, --version and mistakes print and exit

let region = args.value("region").unwrap_or("eu-west-2");
let replicas: u32 = args.value_as("replicas").unwrap_or(1);
```

It does what most programs need (flags, options, arguments, one level of subcommands,
values parsed into types) and no more; for rules between flags or anything fancier, clap
is the one to reach for.

## Help that looks like the rest of your CLI

With a short entrance on a terminal: lines fade in one after another while the name
shimmers.

```rust
tinsel::Help::new("deploy", "ship an app to the cloud")
    .version(env!("CARGO_PKG_VERSION"))
    .usage("deploy [OPTIONS] <APP>")
    .command("status", "what's running where")
    .flag("--region <NAME>", "where to deploy")
    .example("deploy my-app --region us-east-1", "ship to the US east region")
    .animate();
```

## For scripts and tools

One switch from a `--json` or `--quiet` flag:

```rust
tinsel::set_output(tinsel::Output::Json);   // or TINSEL_OUTPUT=json
```

When stdout carries data for another program, draw on stderr instead:

```rust
tinsel::set_target(tinsel::Target::Stderr);   // or TINSEL_TARGET=stderr
```
