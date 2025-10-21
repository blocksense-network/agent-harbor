# How to use clap (Rust)

`clap` is the de-facto standard command-line argument parser for Rust. It offers a powerful and ergonomic API via either derive macros or a builder pattern. This guide covers common patterns and advanced features in `clap 4.x`.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
# Optional, for shell completions generation
clap_complete = "4"
# Optional, for man page generation
clap_mangen = "0.2"
```

Or with `cargo add`:

```bash
cargo add clap --features derive
cargo add clap_complete --optional
cargo add clap_mangen --optional
```

## Quickstart (derive API)

```rust
use clap::{Parser, Subcommand, Args, ValueEnum};
use std::{net::IpAddr, path::PathBuf, process::ExitCode};

/// Example CLI showcasing common clap patterns
#[derive(Debug, Parser)]
#[command(
    name = "acme",
    about = "Acme CLI",
    version,
    author,
    propagate_version = true,
    long_about = None,
    // Show help if no args are provided
    arg_required_else_help = true,
)]
struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Subcommands
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print a greeting
    Greet(GreetArgs),

    /// Start the server
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
struct GreetArgs {
    /// Who to greet
    #[arg(short, long, default_value = "world")]
    name: String,

    /// Greeting style
    #[arg(long, value_enum, env = "ACME_STYLE", default_value_t = Style::Friendly)]
    style: Style,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
enum Style { Friendly, Shouty }

#[derive(Debug, Args)]
struct ServeArgs {
    /// Port to listen on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Optional config file path
    #[arg(long, value_hint = clap::builder::ValueHint::FilePath)]
    config: Option<PathBuf>,

    /// Allowed client IPs (comma-delimited or repeated)
    #[arg(long, num_args = 1.., value_delimiter = ',', value_parser = clap::value_parser!(IpAddr))]
    allow_ip: Vec<IpAddr>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Greet(args) => {
            let mut msg = format!("Hello, {}!", args.name);
            if matches!(args.style, Style::Shouty) {
                msg = msg.to_uppercase();
            }
            if cli.verbose > 0 {
                eprintln!("verbosity={}", cli.verbose);
            }
            println!("{}", msg);
        }
        Commands::Serve(args) => {
            if cli.verbose > 0 {
                eprintln!("Starting server on port {}", args.port);
            }
            if let Some(path) = args.config.as_deref() {
                eprintln!("Using config: {}", path.display());
            }
            if !args.allow_ip.is_empty() {
                eprintln!("Allow list: {:?}", args.allow_ip);
            }
        }
    }

    ExitCode::SUCCESS
}
```

### Notes on the derive API

- Struct doc comments become command `about`/`long_about`. Field doc comments become `help`/`long_help`.
- Long flag names default to `kebab-case` from the field name (e.g. `serve_port` → `--serve-port`).
- `bool` flags default to `action = ArgAction::SetTrue`. For `-vvv` style verbosity, use `ArgAction::Count` on an integer field.
- Use `default_value_t` for typed defaults, or `default_value` (string) in the builder API.
- Use `value_enum` to accept a restricted set of values via a Rust `enum`.
- `num_args = 1..` accepts a variable number of values; combine with `value_delimiter` for comma-separated inputs.
- `value_hint` improves shell completions (e.g. `ValueHint::FilePath`).
- `env = "VAR"` lets environment variables provide defaults.

## Builder API (Command/Arg)

```rust
use clap::{value_parser, Arg, ArgAction, Command};
use clap::builder::ValueHint;
use std::net::IpAddr;

fn build_cli() -> Command {
    Command::new("acme")
        .about("Acme CLI")
        .version(env!("CARGO_PKG_VERSION"))
        .arg_required_else_help(true)
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::Count)
                .help("Increase verbosity (-v, -vv, -vvv)"),
        )
        .subcommand(
            Command::new("serve")
                .about("Start the server")
                .arg(
                    Arg::new("port")
                        .short('p')
                        .long("port")
                        .value_parser(value_parser!(u16))
                        .default_value("8080"),
                )
                .arg(
                    Arg::new("config")
                        .long("config")
                        .value_hint(ValueHint::FilePath),
                )
                .arg(
                    Arg::new("allow-ip")
                        .long("allow-ip")
                        .num_args(1..)
                        .value_delimiter(',')
                        .value_parser(value_parser!(IpAddr)),
                ),
        )
}

fn main() {
    let matches = build_cli().get_matches();

    let verbose = matches.get_count("verbose");
    if let Some(("serve", sub_m)) = matches.subcommand() {
        let port = *sub_m.get_one::<u16>("port").expect("has default");
        let config = sub_m.get_one::<String>("config");
        let allow: Vec<IpAddr> = sub_m
            .get_many::<IpAddr>("allow-ip")
            .map(|vals| vals.copied().collect())
            .unwrap_or_default();
        eprintln!("v={}, port={}, cfg={:?}, allow={:?}", verbose, port, config, allow);
    }
}
```

### Retrieving values (typed)

- One value: `matches.get_one::<T>("name") -> Option<&T>`
- Many values: `matches.get_many::<T>("name") -> Option<ValuesRef<T>>`
- Flags with `ArgAction::Count`: `matches.get_count("verbose") -> u8`
- Boolean flags: `matches.get_flag("quiet") -> bool`
- Consume ownership: `matches.remove_one::<T>("name") -> Option<T>`

## Subcommands and shared options

- Model subcommands with `#[derive(Subcommand)] enum` and attach `#[arg(...)]` to per-subcommand structs.
- Share options across subcommands with `#[derive(Args)]` and `#[command(flatten)]`.

```rust
#[derive(Args, Debug)]
struct GlobalNetwork {
    /// HTTP proxy to use
    #[arg(long, env = "HTTP_PROXY")] proxy: Option<String>,
}

#[derive(Parser, Debug)]
struct Cli {
    #[command(flatten)]
    net: GlobalNetwork,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd { Fetch { url: String }, Push { url: String } }
```

## Mutually-exclusive and required groups

- Builder API: `ArgGroup::new("format").args(["json", "yaml"]).required(true)`
- Derive API: place both args in the same group name.

```rust
use clap::{Args, Parser};

#[derive(Parser, Debug)]
struct Cli {
    /// Output as JSON
    #[arg(long, group = "format")] json: bool,
    /// Output as YAML
    #[arg(long, group = "format")] yaml: bool,
}
```

You can also use `conflicts_with`, `conflicts_with_all`, `requires`, and `requires_if` for fine-grained rules.

## Parsing and validation

- `value_parser!(T)` parses into common types (e.g. `u16`, `IpAddr`, `PathBuf`).
- Custom parsing: implement `FromStr` on your type and use `value_parser!(YourType)` or `#[arg(value_parser = your_parser_fn)]`.
- Constrain values with `value_parser!(T).range(1024..65535)` (builder) or attributes like `#[arg(value_parser = clap::value_parser!(u16).range(1024..))]`.

## Environment variables and defaults

- `#[arg(env = "VAR")]` or builder `.env("VAR")` sets an environment-backed default.
- `default_value_t = 8080` (derive) or `.default_value("8080")` (builder) sets defaults.
- `default_missing_value = "..."` lets a flag take an optional value (`--opt` equals `--opt=...`).

## Help text and metadata

- Use doc comments for automatic `help` and `about`/`long_about`.
- Macros like `version`, `author`, `name`, and `about` can be derived from Cargo with `#[command(version, author, about)]` or builder equivalents.
- Style: control color/help behavior with `.color(clap::ColorChoice::Auto)` and `.disable_help_subcommand(true)` etc.

## Error handling

- `get_matches()` prints an error/help then exits on failure.
- `try_get_matches()` returns `Result<ArgMatches, Error>` so you can handle errors yourself.
- In derive: `Cli::parse()` vs `Cli::try_parse()`.

## Shell completions (optional)

Use `clap_complete` to generate completion scripts.

```rust
use clap::{CommandFactory, Parser};
use clap_complete::{generate, shells::Bash};
use std::io;

#[derive(Parser)]
struct Cli { /* ... */ }

fn main() {
    let mut cmd = Cli::command();
    // Generate Bash completions to stdout:
    generate(Bash, &mut cmd, cmd.get_name(), &mut io::stdout());
}
```

- Replace `Bash` with `Zsh`, `Fish`, `PowerShell`, or `Elvish` as needed.
- You can wire a `completions` subcommand to print the appropriate script.

### How completion generation works

- `clap_complete` inspects your `clap::Command` tree and emits a shell script that knows your commands, subcommands, flags, options, and value choices.
- Value choices come from:
  - `ValueEnum` derives
  - `PossibleValuesParser` (builder) / `#[arg(value_enum)]` (derive)
  - Path and other hints via `ValueHint` (e.g., `FilePath`, `DirPath`)
- The generated scripts are static: shells don’t call your binary to compute completions at runtime by default. Prefer encoding choices in your CLI definition.

### Installation (per shell)

- Bash (requires bash-completion installed):
  - User: `mkdir -p ~/.local/share/bash-completion/completions && yourapp completions bash > ~/.local/share/bash-completion/completions/yourapp`
  - System: `yourapp completions bash | sudo tee /etc/bash_completion.d/yourapp >/dev/null`
  - Alternatively for current session: `source <(yourapp completions bash)`
- Zsh:
  - Generate: `yourapp completions zsh > ~/.zsh/completions/_yourapp`
  - Ensure in `~/.zshrc`:
    - `fpath=(~/.zsh/completions $fpath)`
    - `autoload -U compinit && compinit`
- Fish:
  - `mkdir -p ~/.config/fish/completions && yourapp completions fish > ~/.config/fish/completions/yourapp.fish`
  - Fish will auto-load from that directory.
- PowerShell:
  - Current session: `yourapp completions powershell | Out-String | Invoke-Expression`
  - Persist (profile): `yourapp completions powershell | Out-String | Add-Content -Path $PROFILE`

### Build-time generation (optional)

If packaging your app, generate scripts during build and install them into standard locations.

```rust
// build.rs
use clap::{Command, Arg};
use clap_complete::{generate_to, Shell};
use std::{env, fs, path::PathBuf};

fn build_cli() -> Command {
    Command::new("yourapp").arg(Arg::new("example"))
}

fn main() {
    let mut cmd = build_cli();
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("completions");
    fs::create_dir_all(&out_dir).unwrap();
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
        generate_to(shell, &mut cmd, cmd.get_name(), &out_dir).unwrap();
    }
}
```

Package installers can then place the files appropriately (e.g., `/etc/bash_completion.d/`, `share/zsh/site-functions/_yourapp`, `~/.config/fish/completions/yourapp.fish`).

### Customizing completions

- Enumerated values: prefer `#[arg(value_enum)]` on `ValueEnum` or use `PossibleValuesParser` for an explicit list.
- Paths and commands: set `#[arg(value_hint = ...)]` to improve shell behavior (e.g., path expansion).
- Dynamic, runtime completers: not supported by `clap_complete`’s default scripts across shells. If you require dynamic suggestions, consider shell-specific mechanisms (e.g., Fish functions calling your binary) or maintaining a custom generator; keep in mind portability and maintenance costs.

## Man pages (optional)

Use `clap_mangen` to generate man pages.

```rust
use clap::{CommandFactory, Parser};
use clap_mangen::Man;
use std::io::Write;

#[derive(Parser)]
struct Cli { /* ... */ }

fn main() -> std::io::Result<()> {
    let cmd = Cli::command();
    let man = Man::new(cmd);
    let mut out = Vec::new();
    man.render(&mut out)?;
    std::fs::write("acme.1", out)?;
    Ok(())
}
```

## Testing your CLI

- Validate your CLI structure with `debug_assert()`:

```rust
build_cli().debug_assert();
```

- Test parsing without running the program:

```rust
#[test]
fn parse_serve_args() {
    let m = build_cli()
        .try_get_matches_from(["acme", "serve", "--port", "9090", "--allow-ip", "127.0.0.1"])
        .unwrap();
    let ("serve", sm) = m.subcommand().unwrap();
    assert_eq!(*sm.get_one::<u16>("port").unwrap(), 9090);
}
```

## Common tips and pitfalls

- Prefer `derive` for ergonomics; use `builder` for dynamic CLIs.
- Use `global = true` on flags that should apply to all subcommands (e.g. `--verbose`).
- For list inputs, prefer `num_args = 1..` + `value_delimiter` to support both repeated and comma-delimited values.
- Use `ValueEnum` for enums to auto-generate valid choices and help text.
- Use `ValueHint` to improve completion UX for paths, commands, hostnames, etc.
- Migrate from `clap 3` to `4` by replacing `App` with `Command` and typed getters like `get_one::<T>`.

---

Further reading: `clap` docs on `docs.rs` and the official examples repository provide extensive, up-to-date examples beyond this guide.

## Idiomatic patterns for production CLIs

### Verbosity and quiet flags

Use `clap-verbosity-flag` to standardize `-v/-q` handling.

```toml
[dependencies]
clap-verbosity-flag = "2"
```

```rust
use clap::Parser;
use clap_verbosity_flag::{Verbosity, InfoLevel};

#[derive(Debug, Parser)]
struct Cli {
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,

    /// Emit machine-readable JSON
    #[arg(long)]
    json: bool,
}

fn main() {
    let cli = Cli::parse();
    // Map cli.verbose to your logger of choice
    // e.g., env_logger or tracing subscriber
}
```

### Propagate version to subcommands

Add `propagate_version = true` (already shown in the quickstart) so `-V/--version` works on subcommands.

### Completions subcommand pattern

Wire a dedicated command group for shell completions, matching `ah shell-completion script [shell]` behavior in the CLI spec (plus `install` and `complete`).

Note:

- The helper functions shown below (`detect_shell`, `install_to_default`, `read_shell_line_and_cursor`, `compute_dynamic_suggestions`) are application-provided utilities. They are not part of `clap`/`clap_complete`.
- `clap_complete` generates static scripts by default. To support dynamic, runtime suggestions, you must customize the installed completion script to call back into your binary (e.g., invoke `ah shell-completion complete ...`) using shell-specific mechanisms (bash `complete -C`, zsh functions, fish `complete --command` with a wrapper function).

```rust
use clap::{Parser, Subcommand, CommandFactory};
use clap_complete::{generate, Shell};

#[derive(Debug, Parser)]
#[command(version, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Shell completion utilities
    #[command(subcommand)]
    ShellCompletion(ShellCompletionCmd),
    // other subcommands...
}

#[derive(Debug, Subcommand)]
enum ShellCompletionCmd {
    /// Print completion script to stdout
    Script { shell: Option<Shell> },
    /// Install completion script into standard user location
    Install {
        #[arg(long)] shell: Option<Shell>,
        #[arg(long)] dest: Option<std::path::PathBuf>,
        #[arg(long)] force: bool,
    },
    /// Emit dynamic completion candidates for the current line
    Complete {
        #[arg(long)] shell: Option<Shell>,
        #[arg(long)] line: Option<String>,
        #[arg(long)] cursor: Option<usize>,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::ShellCompletion(sub) => match sub {
            ShellCompletionCmd::Script { shell } => {
                let mut cmd = Cli::command();
                let shell = shell.unwrap_or(detect_shell());
                generate(shell, &mut cmd, cmd.get_name(), &mut std::io::stdout());
            }
            ShellCompletionCmd::Install { shell, dest, force: _ } => {
                let mut cmd = Cli::command();
                let shell = shell.unwrap_or(detect_shell());
                let mut buf = Vec::new();
                generate(shell, &mut cmd, cmd.get_name(), &mut buf);
                install_to_default(shell, &buf, dest.as_deref()).expect("install failed");
            }
            ShellCompletionCmd::Complete { shell, line, cursor } => {
                let shell = shell.unwrap_or(detect_shell());
                let (line, cursor) = read_shell_line_and_cursor(shell, line, cursor);
                for s in compute_dynamic_suggestions(&line, cursor) {
                    println!("{}", s);
                }
            }
        },
        // ...
    }
}
```

### Dynamic completion hooks per shell

`clap_complete` generates static scripts. For runtime/dynamic suggestions (e.g., listing branches from the current repo), install a shell hook that calls your binary's `ah shell-completion complete` subcommand and prints one suggestion per line. Below are minimal, widely used patterns per shell.

#### Bash

Two viable approaches exist; pick the one you prefer.

Option A: External completer (bash sets `COMP_LINE` and `COMP_POINT` automatically):

```bash
complete -o bashdefault -o default -C 'ah shell-completion complete --shell bash' ah
```

Option B: Function-based completer using `compgen`:

```bash
_ah_complete() {
  local cur
  cur="${COMP_WORDS[COMP_CWORD]}"
  local suggestions
  suggestions=$(ah shell-completion complete --shell bash --line "$COMP_LINE" --cursor "$COMP_POINT")
  COMPREPLY=( $(compgen -W "$suggestions" -- "$cur") )
}
complete -F _ah_complete ah
```

In both cases, `ah shell-completion complete` should write newline-delimited candidates to stdout.

#### Zsh

Zsh completion functions can call back into your CLI and then use `_describe` or `compadd`:

```zsh
_ah() {
  local -a suggestions
  suggestions=(${(f)$(ah shell-completion complete --shell zsh --line "$BUFFER" --cursor "$CURSOR")})
  _describe -t commands 'ah suggestions' suggestions
}
compdef _ah ah
```

#### Fish

Fish supports command substitutions for dynamic candidates. Use `commandline` to read the buffer and cursor position:

```fish
function __ah_complete
    set -l line (commandline --current-process)
    set -l cursor (commandline --cursor)
    ah shell-completion complete --shell fish --line "$line" --cursor $cursor
end
complete -c ah -a '(__ah_complete)'
```

#### PowerShell (pwsh)

Register a native argument completer that forwards the current buffer and cursor position:

```powershell
Register-ArgumentCompleter -Native -CommandName ah -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)
  $line = $commandAst.Extent.Text
  ah shell-completion complete --shell pwsh --line $line --cursor $cursorPosition |
    ForEach-Object { [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_) }
}
```

### Reading line and cursor by shell (fallbacks your CLI can honor)

When `--line/--cursor` are omitted, your CLI can read standard shell-provided variables:

- Bash: `COMP_LINE`, `COMP_POINT`
- Zsh: `$BUFFER`, `$CURSOR`
- Fish: `commandline --current-process`, `commandline --cursor`
- PowerShell: Not env-based; use `Register-ArgumentCompleter` parameters (`$commandAst.Extent.Text`, `$cursorPosition`)

### Install directories and detection heuristics

For `ah shell-completion install`, prefer user-level locations and create parents as needed. Common defaults:

- Bash (user): `~/.local/share/bash-completion/completions/ah`
  - macOS (Homebrew): `/opt/homebrew/etc/bash_completion.d/ah` (Apple Silicon), `/usr/local/etc/bash_completion.d/ah` (Intel)
  - System-wide (Linux): `/etc/bash_completion.d/ah` or `/usr/share/bash-completion/completions/ah`
- Zsh (user): `~/.zsh/completions/_ah` and ensure in `~/.zshrc`:
  - `fpath=(~/.zsh/completions $fpath)` then `autoload -U compinit && compinit`
- Fish (user): `~/.config/fish/completions/ah.fish`
- PowerShell: Append to `$PROFILE` to dot-source the script for each session, or register a completer at startup (as shown above).

Heuristics:

- Detect shell via `$SHELL` (POSIX shells) or `$PSMODULEPATH`/`$PROFILE` (PowerShell), with an override flag `--shell`.
- On macOS, detect Homebrew prefix via `brew --prefix` and prefer `$PREFIX/etc/bash_completion.d` when writable.
- Always avoid overwriting existing files unless `--force` is given.

### Minimal Rust helpers for dynamic completion

Your `ah shell-completion complete` can accept `--shell <SHELL>`, `--line <STRING>`, and `--cursor <usize>`, apply fallbacks, and print suggestions:

```rust
use clap::{Parser, Subcommand, CommandFactory};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(version, propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    #[command(subcommand)]
    ShellCompletion(ShellCompletionCmd),
}

#[derive(Debug, Subcommand)]
enum ShellCompletionCmd {
    Complete { #[arg(long)] shell: Option<Shell>, #[arg(long)] line: Option<String>, #[arg(long)] cursor: Option<usize> },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::ShellCompletion(ShellCompletionCmd::Complete { shell, line, cursor }) => {
            let shell = shell.unwrap_or_else(detect_shell);
            let (line, cursor) = resolve_line_and_cursor(shell, line, cursor);
            for s in compute_dynamic_suggestions(&line, cursor) { println!("{}", s); }
        }
        _ => {}
    }
}

fn detect_shell() -> Shell { /* inspect $SHELL or platform; default to Shell::Bash */ Shell::Bash }
fn resolve_line_and_cursor(shell: Shell, line: Option<String>, cursor: Option<usize>) -> (String, usize) {
    if let (Some(l), Some(c)) = (line, cursor) { return (l, c); }
    match shell {
        Shell::Bash => {
            let l = std::env::var("COMP_LINE").unwrap_or_default();
            let c = std::env::var("COMP_POINT").ok().and_then(|v| v.parse().ok()).unwrap_or(l.len());
            (l, c)
        }
        Shell::Zsh => {
            // Zsh vars are in the invoking process; prefer passing --line/--cursor from the script
            (line.unwrap_or_default(), cursor.unwrap_or(0))
        }
        Shell::Fish => {
            (line.unwrap_or_default(), cursor.unwrap_or(0))
        }
        Shell::PowerShell => {
            (line.unwrap_or_default(), cursor.unwrap_or(0))
        }
        _ => (line.unwrap_or_default(), cursor.unwrap_or(0)),
    }
}

fn compute_dynamic_suggestions(line: &str, cursor: usize) -> Vec<String> {
    // Your logic: tokenize, inspect context, list branches/agents/etc.
    let _ = (line, cursor);
    Vec::new()
}
```

This section focuses on wiring. See the CLI spec for the specific dynamic sources (e.g., branch names, workspaces) to return.

```

### Environment variable prefix strategy

- Clap supports per-argument environment variables via `#[arg(env = "NAME")]` or builder `.env("NAME")`.
- It does not auto-apply an env prefix across all args; define env vars per arg using a consistent `AH_` prefix (e.g., `AH_REPO`, `AH_JSON`).
- If you prefer centralization, set env names programmatically using the builder API via `Cli::command()` and updating each `Arg`.
```
