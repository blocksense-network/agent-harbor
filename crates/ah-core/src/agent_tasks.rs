//! Agent task file management and operations.
//!
//! This module provides functionality for managing agent task files in VCS repositories,
//! including creating initial tasks, appending follow-up tasks, and detecting task branches.
//! This is a direct port of the Ruby AgentTasks class functionality.

use ah_repo::{VcsRepo, VcsResult};
use chrono::{DateTime, Datelike, Timelike, Utc};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Manages agent task files in a VCS repository.
///
/// This struct provides methods for creating, managing, and detecting agent task files
/// within a VCS repository structure. Task files are stored in `.agents/tasks/YYYY/MM/`
/// directories with timestamped filenames.
#[derive(Debug)]
pub struct AgentTasks {
    /// The VCS repository this AgentTasks instance operates on.
    repo: VcsRepo,
}

impl AgentTasks {
    /// Create a new AgentTasks instance for the repository at the given path.
    ///
    /// # Arguments
    /// * `path_in_repo` - Path within or to a VCS repository. Defaults to current directory.
    ///
    /// # Errors
    /// Returns an error if the path is not within a VCS repository.
    pub fn new<P: AsRef<Path>>(path_in_repo: P) -> VcsResult<Self> {
        let repo = VcsRepo::new(path_in_repo)?;
        Ok(Self { repo })
    }

    /// Get the path to the agent task file in the current branch.
    ///
    /// This method finds the task file that was created when the current agent task branch
    /// was started. It does this by looking at the latest agent branch commit and finding
    /// the single file that was introduced in that commit.
    ///
    /// # Returns
    /// The absolute path to the task file.
    ///
    /// # Errors
    /// Returns an error if not currently on an agent task branch, or if the task start
    /// commit doesn't contain exactly one file.
    pub fn agent_task_file_in_current_branch(&self) -> VcsResult<PathBuf> {
        let start_commit_hash = self.repo.latest_agent_branch_commit()?;
        if start_commit_hash.is_empty() {
            return Err(ah_repo::VcsError::Other(
                "You are not currently on an agent task branch".into(),
            ));
        }

        let files_in_commit = self.repo.files_in_commit(&start_commit_hash)?;
        if files_in_commit.is_empty() {
            return Err(ah_repo::VcsError::Other(format!(
                "Error: No files found in the task start commit ('{}').",
                start_commit_hash
            )));
        }

        Ok(self.repo.root().join(&files_in_commit[0]))
    }

    /// Check if the current branch is an agent task branch.
    ///
    /// # Returns
    /// `true` if the current branch is an agent task branch, `false` otherwise.
    pub fn on_task_branch(&self) -> VcsResult<bool> {
        match self.repo.latest_agent_branch_commit() {
            Ok(commit) => Ok(!commit.is_empty()),
            Err(_) => Ok(false),
        }
    }

    /// Record an initial task with the given content and branch name.
    ///
    /// This creates a new task file with timestamped naming in the `.agents/tasks/`
    /// directory structure and commits it to the VCS repository.
    ///
    /// # Arguments
    /// * `task_content` - The content of the task to record.
    /// * `branch_name` - The name of the agent branch.
    /// * `devshell` - Optional devshell name to include in commit message.
    ///
    /// # Errors
    /// Returns an error if file creation or VCS operations fail.
    pub fn record_initial_task(
        &self,
        task_content: &str,
        branch_name: &str,
        devshell: Option<&str>,
    ) -> VcsResult<()> {
        let now: DateTime<Utc> = Utc::now();
        let year = now.year();
        let month = format!("{:02}", now.month());
        let day = format!("{:02}", now.day());
        let hour = format!("{:02}", now.hour());
        let min = format!("{:02}", now.minute());

        let tasks_dir = self
            .repo
            .root()
            .join(".agents")
            .join("tasks")
            .join(year.to_string())
            .join(month);
        fs::create_dir_all(&tasks_dir)?;

        let filename = format!("{}-{}{}-{}", day, hour, min, branch_name);
        let task_file = tasks_dir.join(filename);

        let mut commit_msg = format!("Start-Agent-Branch: {}", branch_name);

        if let Some(remote_url) = self.repo.default_remote_http_url()? {
            commit_msg.push_str(&format!("\nTarget-Remote: {}", remote_url));
        }

        if let Some(devshell_name) = devshell {
            commit_msg.push_str(&format!("\nDev-Shell: {}", devshell_name));
        }

        fs::write(&task_file, task_content)?;
        self.repo.commit_file(task_file.to_str().unwrap(), &commit_msg)?;

        Ok(())
    }

    /// Append a follow-up task to the existing task file.
    ///
    /// This method finds the current task file (from the initial task commit) and
    /// appends the new task content with the standard delimiter.
    ///
    /// # Arguments
    /// * `task_content` - The content of the follow-up task to append.
    ///
    /// # Errors
    /// Returns an error if not on a task branch, or if file operations fail.
    pub fn append_task(&self, task_content: &str) -> VcsResult<()> {
        let start_commit = self.repo.latest_agent_branch_commit()?;
        if start_commit.is_empty() {
            return Err(ah_repo::VcsError::Other(
                "Error: Could not locate task start commit".into(),
            ));
        }

        let files = self.repo.files_in_commit(&start_commit)?;
        if files.len() != 1 {
            return Err(ah_repo::VcsError::Other(
                "Error: Task start commit should introduce exactly one file".into(),
            ));
        }

        let task_file = self.repo.root().join(&files[0]);

        let mut file = fs::OpenOptions::new().append(true).open(&task_file)?;

        write!(file, "\n--- FOLLOW UP TASK ---\n{}", task_content)?;

        self.repo.commit_file(task_file.to_str().unwrap(), "Follow-up task")?;

        Ok(())
    }

    /// Check if the system has internet connectivity.
    ///
    /// Uses Google's connectivity check service to determine if internet access is available.
    ///
    /// # Returns
    /// `true` if internet is available, `false` otherwise.
    pub fn online(&self) -> bool {
        // Use Google's connectivity check service - a lightweight endpoint designed for connectivity testing
        // This service is globally distributed and operated by Google, making it highly reliable
        // Reference: https://developers.google.com/speed/public-dns/docs/doh
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(3)).build();

        match agent.get("http://connectivitycheck.gstatic.com/generate_204").call() {
            Ok(response) => response.status() == 204,
            Err(_) => false,
        }
    }

    /// Set up autopush for the current task branch.
    ///
    /// Extracts the target remote and branch information from the initial task commit
    /// message and configures autopush.
    ///
    /// # Errors
    /// Returns an error if not on a task branch, or if commit message parsing fails.
    pub fn setup_autopush(&self) -> VcsResult<()> {
        let first_commit_hash = self.repo.latest_agent_branch_commit()?;
        if first_commit_hash.is_empty() {
            return Err(ah_repo::VcsError::Other(
                "Error: Could not find first commit in current branch".into(),
            ));
        }

        let commit_msg = self.repo.commit_message(&first_commit_hash)?;
        let commit_msg = match commit_msg {
            Some(msg) => msg,
            None => {
                return Err(ah_repo::VcsError::Other(
                    "Error: Could not retrieve commit message from first commit".into(),
                ))
            }
        };

        let remote_match = commit_msg
            .lines()
            .find(|line| line.starts_with("Target-Remote:"))
            .and_then(|line| line.strip_prefix("Target-Remote:").map(str::trim));

        let target_remote = match remote_match {
            Some(remote) if !remote.is_empty() => remote,
            _ => {
                return Err(ah_repo::VcsError::Other(
                    "Error: Target-Remote not found in commit message".into(),
                ))
            }
        };

        let branch_match = commit_msg
            .lines()
            .find(|line| line.starts_with("Start-Agent-Branch:"))
            .and_then(|line| line.strip_prefix("Start-Agent-Branch:").map(str::trim));

        let target_branch = match branch_match {
            Some(branch) if !branch.is_empty() => branch,
            _ => {
                return Err(ah_repo::VcsError::Other(
                    "Error: Start-Agent-Branch not found in commit message".into(),
                ))
            }
        };

        self.repo.setup_autopush(target_remote, target_branch)?;

        Ok(())
    }

    /// Get the VCS repository instance.
    pub fn repo(&self) -> &VcsRepo {
        &self.repo
    }
}
