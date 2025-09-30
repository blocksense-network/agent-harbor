use clap::Parser;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path(s) to the dynamic library/libraries to inject (colon-separated)
    #[arg(short, long, value_name = "LIBRARY_PATH")]
    library: String,

    /// Command to execute with library injection
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.command.is_empty() {
        eprintln!("Error: No command specified");
        std::process::exit(1);
    }

    // Check if all libraries exist
    let lib_paths: Vec<&str> = args.library.split(':').collect();
    for lib_path_str in &lib_paths {
        let lib_path = PathBuf::from(lib_path_str);
        if !lib_path.exists() {
            eprintln!("Error: Library file does not exist: {}", lib_path.display());
            std::process::exit(1);
        }
    }

    // Build the command
    let mut cmd = Command::new(&args.command[0]);
    cmd.args(&args.command[1..]);

    // Set DYLD_INSERT_LIBRARIES with colon-separated paths
    cmd.env("DYLD_INSERT_LIBRARIES", args.library);

    // Configure stdio to inherit from parent
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    // Execute the command
    let status = cmd.status()?;

    // Exit with the same status as the child process
    std::process::exit(status.code().unwrap_or(1));
}
