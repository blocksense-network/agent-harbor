use std::fs;
use std::path::Path;
use std::process::Stdio;
use tempfile::TempDir;

use ah_repo::{VcsError, VcsRepo, VcsType};

fn check_git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn setup_git_repo() -> Result<(TempDir, TempDir), Box<dyn std::error::Error>> {
    // Set environment variables globally for this test
    std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    std::env::set_var("GIT_TERMINAL_PROMPT", "0");
    std::env::set_var("GIT_ASKPASS", "echo");
    std::env::set_var("SSH_ASKPASS", "echo");

    // Set HOME to a temporary directory to avoid accessing user git/ssh config
    let temp_home = TempDir::new()?;
    std::env::set_var("HOME", temp_home.path());

    let remote_dir = TempDir::new()?;
    let repo_dir = TempDir::new()?;

    // Initialize bare remote repository
    std::process::Command::new("git")
        .args(&["init", "--bare"])
        .current_dir(&remote_dir)
        .output()?;

    // Initialize local repository
    std::process::Command::new("git")
        .args(&["init", "-b", "main"])
        .current_dir(&repo_dir)
        .output()?;

    // Configure git
    std::process::Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(&repo_dir)
        .output()?;

    std::process::Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(&repo_dir)
        .output()?;

    // Create initial file and commit
    fs::write(repo_dir.path().join("README.md"), "Initial content")?;
    std::process::Command::new("git")
        .args(&["add", "README.md"])
        .current_dir(&repo_dir)
        .output()?;

    std::process::Command::new("git")
        .args(&["commit", "-m", "Initial commit"])
        .current_dir(&repo_dir)
        .output()?;

    // Don't add remote for now to avoid potential issues

    Ok((temp_home, repo_dir))
}

#[test]
fn test_repository_detection() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let repo_path = repo.path().join("some").join("nested").join("dir");
    fs::create_dir_all(&repo_path).unwrap();

    let vcs_repo = VcsRepo::new(&repo_path).unwrap();
    assert_eq!(
        vcs_repo.root().canonicalize().unwrap(),
        repo.path().canonicalize().unwrap()
    );
    assert_eq!(vcs_repo.vcs_type(), VcsType::Git);
}

#[test]
fn test_current_branch() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();

    let vcs_repo = VcsRepo::new(repo.path()).unwrap();
    let branch = vcs_repo.current_branch().unwrap();
    assert_eq!(branch, "main");
}

#[test]
fn test_branch_validation() {
    // Valid branch names
    assert!(VcsRepo::valid_branch_name("feature-branch"));
    assert!(VcsRepo::valid_branch_name("bug_fix"));
    assert!(VcsRepo::valid_branch_name("v1.0.0"));
    assert!(VcsRepo::valid_branch_name("test_branch"));

    // Invalid branch names
    assert!(!VcsRepo::valid_branch_name("feature branch")); // space
    assert!(!VcsRepo::valid_branch_name("feature/branch")); // slash
    assert!(!VcsRepo::valid_branch_name("feature@branch")); // @ symbol
}

#[test]
fn test_protected_branches() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    assert!(vcs_repo.is_protected_branch("main"));
    assert!(vcs_repo.is_protected_branch("master"));
    assert!(vcs_repo.is_protected_branch("trunk"));
    assert!(vcs_repo.is_protected_branch("default"));
    assert!(!vcs_repo.is_protected_branch("feature-branch"));
}

#[test]
fn test_start_branch() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Test starting a new branch
    vcs_repo.start_branch("feature-test").unwrap();

    // Verify we're on the new branch
    let current_branch = vcs_repo.current_branch().unwrap();
    assert_eq!(current_branch, "feature-test");

    // Test that we can't start a protected branch
    let result = vcs_repo.start_branch("main");
    assert!(matches!(result, Err(VcsError::ProtectedBranch(_))));
}

#[test]
fn test_commit_file() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Start a new branch
    vcs_repo.start_branch("test-commit").unwrap();

    // Create and commit a new file
    let test_file = repo.path().join("test.txt");
    fs::write(&test_file, "Test content").unwrap();

    vcs_repo.commit_file("test.txt", "Add test file").unwrap();

    // Verify file was committed
    let status = vcs_repo.working_copy_status().unwrap();
    assert!(!status.contains("test.txt"));
}

#[test]
fn test_branches() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Create a few branches
    vcs_repo.start_branch("branch1").unwrap();
    vcs_repo.checkout_branch("main").unwrap();
    vcs_repo.start_branch("branch2").unwrap();

    let branches = vcs_repo.branches().unwrap();
    assert!(branches.contains(&"main".to_string()));
    assert!(branches.contains(&"branch1".to_string()));
    assert!(branches.contains(&"branch2".to_string()));
}

#[test]
fn test_default_remote_http_url() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Test HTTPS URL - add remote first
    std::process::Command::new("git")
        .args(&[
            "remote",
            "add",
            "origin",
            "https://github.com/user/repo.git",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let url = vcs_repo.default_remote_http_url().unwrap();
    assert_eq!(url, Some("https://github.com/user/repo.git".to_string()));

    // Test SSH URL conversion
    std::process::Command::new("git")
        .args(&[
            "remote",
            "set-url",
            "origin",
            "git@github.com:user/repo.git",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();

    let url = vcs_repo.default_remote_http_url().unwrap();
    assert_eq!(url, Some("https://github.com/user/repo.git".to_string()));
}

#[test]
fn test_repository_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let result = VcsRepo::new(temp_dir.path());
    assert!(matches!(result, Err(VcsError::RepositoryNotFound(_))));
}

#[test]
fn test_invalid_branch_name() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_remote, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    let result = vcs_repo.start_branch("invalid branch");
    assert!(matches!(result, Err(VcsError::InvalidBranchName(_))));
}
