use std::fs;
use std::path::Path;
use std::process::Stdio;
use tempfile::TempDir;
use futures::StreamExt;
use tokio::time::{self, Duration};

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

#[tokio::test]
async fn test_stream_tracked_files_basic() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Add a few more files to test streaming
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn lib() {}").unwrap();
    fs::write(repo.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(repo.path().join("Cargo.toml"), r#"[package]
name = "test"
version = "0.1.0"
edition = "2021""#).unwrap();

    // Add and commit the files
    std::process::Command::new("git")
        .args(&["add", "."])
        .current_dir(repo.path())
        .output().unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "Add source files"])
        .current_dir(repo.path())
        .output().unwrap();

    let mut stream = vcs_repo.stream_tracked_files().await.unwrap();

    let mut files = Vec::new();
    while let Some(result) = stream.next().await {
        files.push(result.unwrap());
    }

    // Should have at least README.md and the files we added
    assert!(files.len() >= 3);

    // Check that all files are present (order may vary)
    let file_names: Vec<_> = files.into_iter().map(|f| f).collect();
    let has_readme = file_names.iter().any(|f| f == "README.md");
    let has_lib = file_names.iter().any(|f| f == "src/lib.rs");
    let has_main = file_names.iter().any(|f| f == "src/main.rs");
    let has_cargo = file_names.iter().any(|f| f == "Cargo.toml");

    assert!(has_readme, "README.md should be tracked");
    assert!(has_lib, "src/lib.rs should be tracked");
    assert!(has_main, "src/main.rs should be tracked");
    assert!(has_cargo, "Cargo.toml should be tracked");
}

#[tokio::test]
async fn test_stream_tracked_files_empty_repo() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    let mut stream = vcs_repo.stream_tracked_files().await.unwrap();

    let mut files = Vec::new();
    while let Some(result) = stream.next().await {
        files.push(result.unwrap());
    }

    // Should only have README.md
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], "README.md");
}

#[tokio::test]
async fn test_stream_tracked_files_partial_collection() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Add multiple files
    fs::write(repo.path().join("file1.txt"), "content1").unwrap();
    fs::write(repo.path().join("file2.txt"), "content2").unwrap();
    fs::write(repo.path().join("file3.txt"), "content3").unwrap();

    std::process::Command::new("git")
        .args(&["add", "."])
        .current_dir(repo.path())
        .output().unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "Add test files"])
        .current_dir(repo.path())
        .output().unwrap();

    let mut stream = vcs_repo.stream_tracked_files().await.unwrap();

    // Take only first two files
    let first = stream.next().await.unwrap().unwrap();
    let second = stream.next().await.unwrap().unwrap();

    // Verify we got valid files
    assert!(first.ends_with(".txt") || first == "README.md");
    assert!(second.ends_with(".txt") || second == "README.md");
    assert_ne!(first, second);

    // Stream should still have more files
    let third = stream.next().await.unwrap().unwrap();
    assert!(third.ends_with(".txt") || third == "README.md");
}

#[tokio::test]
async fn test_stream_tracked_files_with_nested_dirs() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Create nested directory structure
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::create_dir_all(repo.path().join("tests")).unwrap();
    fs::create_dir_all(repo.path().join("src/deep/nested")).unwrap();
    fs::write(repo.path().join("src/deep/nested/file.rs"), "content").unwrap();
    fs::write(repo.path().join("tests/integration_test.rs"), "test").unwrap();

    std::process::Command::new("git")
        .args(&["add", "."])
        .current_dir(repo.path())
        .output().unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "Add nested files"])
        .current_dir(repo.path())
        .output().unwrap();

    let stream = vcs_repo.stream_tracked_files().await.unwrap();
    let files: Vec<_> = stream.collect().await;

    assert!(files.len() >= 3); // README.md + 2 new files

    let file_names: Vec<_> = files.into_iter().map(|r| r.unwrap()).collect();

    assert!(file_names.contains(&"src/deep/nested/file.rs".to_string()));
    assert!(file_names.contains(&"tests/integration_test.rs".to_string()));
    assert!(file_names.contains(&"README.md".to_string()));
}

#[tokio::test]
async fn test_stream_tracked_files_non_git_repo() {
    let temp_dir = TempDir::new().unwrap();
    // For non-git repos, VcsRepo::new will fail, so we need to create a mock repo
    // Let's create a temp dir that looks like it has a git repo but doesn't
    let fake_repo_dir = temp_dir.path().join("fake_repo");
    std::fs::create_dir(&fake_repo_dir).unwrap();

    // Create a VcsRepo instance manually for testing (this is a bit of a hack for testing)
    let vcs_repo = VcsRepo {
        root: fake_repo_dir,
        vcs_type: ah_repo::VcsType::Git,
    };

    // This should work but return an empty stream since it's not a git repo
    let result = vcs_repo.stream_tracked_files().await;
    assert!(result.is_ok());

    let stream = result.unwrap();
    let files: Vec<_> = stream.collect().await;

    // Should be empty since it's not a git repo
    assert_eq!(files.len(), 0);
}

#[tokio::test]
async fn test_stream_tracked_files_error_handling() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();

    // Remove the entire .git directory to simulate a corrupted repo
    fs::remove_dir_all(repo.path().join(".git")).unwrap();

    // Now create a VcsRepo manually since VcsRepo::new will fail
    let vcs_repo = VcsRepo {
        root: repo.path().to_path_buf(),
        vcs_type: ah_repo::VcsType::Git,
    };

    // Should work and return empty stream since .git directory doesn't exist
    let result = vcs_repo.stream_tracked_files().await;
    assert!(result.is_ok());

    let stream = result.unwrap();
    let files: Vec<_> = stream.collect().await;

    // Should be empty since .git directory was removed
    assert_eq!(files.len(), 0);
}

#[tokio::test]
async fn test_stream_tracked_files_large_repo() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Create many files to test streaming performance
    for i in 0..100 {
        fs::write(repo.path().join(format!("file_{}.txt", i)), format!("content {}", i)).unwrap();
    }

    std::process::Command::new("git")
        .args(&["add", "."])
        .current_dir(repo.path())
        .output().unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "Add many files"])
        .current_dir(repo.path())
        .output().unwrap();

    let stream = vcs_repo.stream_tracked_files().await.unwrap();
    let files: Vec<_> = stream.collect().await;

    // Should have README.md + 100 files = 101 total
    assert_eq!(files.len(), 101);

    // Check some specific files
    let file_names: Vec<_> = files.into_iter().map(|r| r.unwrap()).collect();
    assert!(file_names.contains(&"README.md".to_string()));
    assert!(file_names.contains(&"file_0.txt".to_string()));
    assert!(file_names.contains(&"file_99.txt".to_string()));
}

#[tokio::test]
async fn test_stream_tracked_files_concurrent_access() {
    if !check_git_available() {
        eprintln!("Git not available, skipping test");
        return;
    }

    let (_temp_home, repo) = setup_git_repo().unwrap();
    let vcs_repo = VcsRepo::new(repo.path()).unwrap();

    // Add a few files
    fs::write(repo.path().join("file1.txt"), "content1").unwrap();
    fs::write(repo.path().join("file2.txt"), "content2").unwrap();

    std::process::Command::new("git")
        .args(&["add", "."])
        .current_dir(repo.path())
        .output().unwrap();
    std::process::Command::new("git")
        .args(&["commit", "-m", "Add files"])
        .current_dir(repo.path())
        .output().unwrap();

    // Test concurrent streaming
    let (result1, result2) = tokio::join!(
        async {
            let stream = vcs_repo.stream_tracked_files().await.unwrap();
            stream.collect::<Vec<_>>().await
        },
        async {
            let stream = vcs_repo.stream_tracked_files().await.unwrap();
            stream.collect::<Vec<_>>().await
        }
    );

    assert!(result1.len() >= 3); // README + 2 files
    assert!(result2.len() >= 3);
    assert_eq!(result1.len(), result2.len());
}
