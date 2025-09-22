use aw_cli::{Cli, Commands, AgentCommands, agent::fs::AgentFsCommands, Parser};

#[test]
fn test_cli_parsing_init_session() {
    let args = vec![
        "aw",
        "agent", "fs", "init-session",
        "--name", "initial-snapshot",
        "--repo", "/path/to/repo",
        "--workspace", "my-workspace"
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs { subcommand: AgentFsCommands::InitSession(_) }
        }
    ));
}

#[test]
fn test_cli_parsing_snapshots() {
    let args = vec![
        "aw",
        "agent", "fs", "snapshots",
        "my-session-id"
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs { subcommand: AgentFsCommands::Snapshots(_) }
        }
    ));
}

#[test]
fn test_cli_parsing_branch_create() {
    let args = vec![
        "aw",
        "agent", "fs", "branch", "create",
        "01HXXXXXXXXXXXXXXXXXXXXX",
        "--name", "test-branch"
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs { subcommand: AgentFsCommands::Branch { .. } }
        }
    ));
}

#[test]
fn test_cli_parsing_branch_bind() {
    let args = vec![
        "aw",
        "agent", "fs", "branch", "bind",
        "01HXXXXXXXXXXXXXXXXXXXXX"
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs { subcommand: AgentFsCommands::Branch { .. } }
        }
    ));
}

#[test]
fn test_cli_parsing_branch_exec() {
    let args = vec![
        "aw",
        "agent", "fs", "branch", "exec",
        "01HXXXXXXXXXXXXXXXXXXXXX",
        "--", "echo", "hello"
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs { subcommand: AgentFsCommands::Branch { .. } }
        }
    ));
}

#[test]
fn test_cli_invalid_command() {
    let args = vec![
        "aw",
        "agent", "fs", "invalid", "command"
    ];

    assert!(Cli::try_parse_from(args).is_err());
}

