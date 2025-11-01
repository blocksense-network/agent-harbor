// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_cli::{AgentCommands, Cli, Commands, Parser, agent::fs::AgentFsCommands};
use ah_test_utils::{TestLogger, logged_test, logged_assert};

#[test]
fn test_cli_parsing_init_session() {
    let mut logger = TestLogger::new("test_cli_parsing_init_session").unwrap();
    
    logger.log("Testing CLI parsing for init-session command").unwrap();
    
    let args = vec![
        "ah",
        "agent",
        "fs",
        "init-session",
        "--name",
        "initial-snapshot",
        "--repo",
        "/path/to/repo",
        "--workspace",
        "my-workspace",
    ];

    logger.log(&format!("Parsing args: {:?}", args)).unwrap();
    
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => {
            logger.log("CLI parsing succeeded").unwrap();
            cli
        },
        Err(e) => {
            logger.finish_failure(&format!("CLI parsing failed: {}", e)).unwrap();
            panic!("CLI parsing failed: {}", e);
        }
    };
    
    logger.log("Checking command structure matches expected pattern").unwrap();
    let matches = matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs {
                subcommand: AgentFsCommands::InitSession(_)
            }
        }
    );
    
    if matches {
        logger.log("Command structure validation passed").unwrap();
        logger.finish_success().unwrap();
    } else {
        logger.finish_failure("Command structure did not match expected pattern").unwrap();
        panic!("Command structure did not match expected pattern");
    }
}

#[test]
fn test_cli_parsing_snapshots() {
    let args = vec!["ah", "agent", "fs", "snapshots", "my-session-id"];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs {
                subcommand: AgentFsCommands::Snapshots(_)
            }
        }
    ));
}

#[test]
fn test_cli_parsing_branch_create() {
    let args = vec![
        "ah",
        "agent",
        "fs",
        "branch",
        "create",
        "01HXXXXXXXXXXXXXXXXXXXXX",
        "--name",
        "test-branch",
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs {
                subcommand: AgentFsCommands::Branch { .. }
            }
        }
    ));
}

#[test]
fn test_cli_parsing_branch_bind() {
    let args = vec![
        "ah",
        "agent",
        "fs",
        "branch",
        "bind",
        "01HXXXXXXXXXXXXXXXXXXXXX",
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs {
                subcommand: AgentFsCommands::Branch { .. }
            }
        }
    ));
}

#[test]
fn test_cli_parsing_branch_exec() {
    let args = vec![
        "ah",
        "agent",
        "fs",
        "branch",
        "exec",
        "01HXXXXXXXXXXXXXXXXXXXXX",
        "--",
        "echo",
        "hello",
    ];

    let cli = Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs {
                subcommand: AgentFsCommands::Branch { .. }
            }
        }
    ));
}

#[test]
fn test_cli_invalid_command() {
    let mut logger = TestLogger::new("test_cli_invalid_command").unwrap();
    
    logger.log("Testing CLI parsing with invalid command - should fail gracefully").unwrap();
    
    let args = vec!["ah", "agent", "fs", "invalid", "command"];
    logger.log(&format!("Parsing invalid args: {:?}", args)).unwrap();

    match Cli::try_parse_from(args) {
        Ok(_) => {
            logger.finish_failure("Expected CLI parsing to fail, but it succeeded").unwrap();
            panic!("Expected CLI parsing to fail for invalid command");
        },
        Err(e) => {
            logger.log(&format!("CLI parsing failed as expected: {}", e)).unwrap();
            logger.log("Verified invalid command rejection works correctly").unwrap();
            logger.finish_success().unwrap();
        }
    }
}

// Example of the simplified macro-based approach
logged_test!(test_cli_parsing_snapshots_with_macro {
    logger.log("Testing CLI parsing for snapshots command using macro").unwrap();
    
    let args = vec!["ah", "agent", "fs", "snapshots", "my-session-id"];
    logger.log(&format!("Parsing args: {:?}", args)).unwrap();

    let cli = Cli::try_parse_from(args).unwrap();
    
    logger.log("Verifying command structure matches expected pattern").unwrap();
    logged_assert!(logger, matches!(
        cli.command,
        Commands::Agent {
            subcommand: AgentCommands::Fs {
                subcommand: AgentFsCommands::Snapshots(_)
            }
        }
    ), "Command should match snapshots pattern");
    
    logger.log("Snapshots command parsing test completed successfully").unwrap();
});
