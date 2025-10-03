use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{VcsError, VcsResult};
use crate::vcs_types::VcsType;

#[derive(Debug)]
pub struct VcsRepo {
    root: PathBuf,
    vcs_type: VcsType,
}

impl VcsRepo {
    /// Create a new VCS repository instance by finding the repository root from the given path
    pub fn new<P: AsRef<Path>>(path_in_repo: P) -> VcsResult<Self> {
        let root = Self::find_repo_root(path_in_repo.as_ref())?;
        let vcs_type = Self::determine_vcs_type(&root)?;

        Ok(Self { root, vcs_type })
    }

    /// Get the repository root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the VCS type
    pub fn vcs_type(&self) -> VcsType {
        self.vcs_type
    }

    /// Get the current branch name
    pub fn current_branch(&self) -> VcsResult<String> {
        let cmd = self.get_current_branch_command();
        let output = self.run_command(&cmd)?;
        let branch = output.trim().to_string();

        if branch.is_empty() {
            return Err(VcsError::Other(
                "Could not determine current branch".to_string(),
            ));
        }

        Ok(branch)
    }

    /// Validate a branch name
    pub fn valid_branch_name(name: &str) -> bool {
        // Branch name validation: alphanumeric, dots, underscores, hyphens only
        let re = Regex::new(r"^[A-Za-z0-9._-]+$").unwrap();
        re.is_match(name)
    }

    /// Check if a branch name is protected
    pub fn is_protected_branch(&self, branch_name: &str) -> bool {
        self.vcs_type.protected_branches().contains(&branch_name)
    }

    /// Start a new branch
    pub fn start_branch(&self, branch_name: &str) -> VcsResult<()> {
        if !Self::valid_branch_name(branch_name) {
            return Err(VcsError::InvalidBranchName(branch_name.to_string()));
        }

        if self.is_protected_branch(branch_name) {
            return Err(VcsError::ProtectedBranch(branch_name.to_string()));
        }

        let commands = self.get_start_branch_commands(branch_name);
        for cmd in commands {
            self.run_command(&cmd)?;
        }

        Ok(())
    }

    /// Commit a file with a message
    pub fn commit_file(&self, file_path: &str, message: &str) -> VcsResult<()> {
        let commands = self.get_commit_file_commands(file_path, message);
        for cmd in commands {
            self.run_command(&cmd)?;
        }
        Ok(())
    }

    /// Push current branch to remote
    pub fn push_current_branch(&self, branch_name: &str, remote: &str) -> VcsResult<()> {
        let cmd = self.get_push_branch_command(branch_name, remote);
        self.run_command(&cmd)?;
        Ok(())
    }

    /// Force push current branch to remote
    pub fn force_push_current_branch(&self, remote: &str, branch: &str) -> VcsResult<()> {
        let cmd = self.get_force_push_branch_command(remote, branch);
        self.run_command(&cmd)?;
        Ok(())
    }

    /// Checkout a branch
    pub fn checkout_branch(&self, branch_name: &str) -> VcsResult<()> {
        if branch_name.trim().is_empty() {
            return Ok(());
        }

        let cmd = self.get_checkout_branch_command(branch_name);
        self.run_command(&cmd)?;
        Ok(())
    }

    /// Create a local branch
    pub fn create_local_branch(&self, branch_name: &str) -> VcsResult<()> {
        let commands = self.get_create_local_branch_commands(branch_name)?;
        for cmd in commands {
            self.run_command(&cmd)?;
        }
        Ok(())
    }

    /// Get the default branch name
    pub fn default_branch(&self) -> &'static str {
        self.vcs_type.default_branch()
    }

    /// Add a file to VCS
    pub fn add_file(&self, file_path: &str) -> VcsResult<()> {
        let cmd = self.get_add_file_command(file_path);
        self.run_command(&cmd)?;
        Ok(())
    }

    /// Get working copy status
    pub fn working_copy_status(&self) -> VcsResult<String> {
        let cmd = self.get_status_command();
        let output = self.run_command(&cmd)?;
        Ok(output.trim().to_string())
    }

    /// Get commit hash for a branch tip
    pub fn tip_commit(&self, branch: &str) -> VcsResult<String> {
        let cmd = self.get_tip_commit_command(branch);
        let output = self.run_command(&cmd)?;
        Ok(output.trim().to_string())
    }

    /// Get commit count between two branches
    pub fn commit_count(&self, base_branch: &str, branch: &str) -> VcsResult<usize> {
        let cmd = self.get_commit_count_command(base_branch, branch);
        let output = self.run_command(&cmd)?;
        output
            .trim()
            .parse()
            .map_err(|_| VcsError::Other("Invalid commit count".to_string()))
    }

    /// Check if branch exists
    pub fn branch_exists(&self, branch_name: &str) -> VcsResult<bool> {
        let branches = self.branches()?;
        Ok(branches.contains(&branch_name.to_string()))
    }

    /// List all branches
    pub fn branches(&self) -> VcsResult<Vec<String>> {
        let cmd = self.get_branches_command();
        let output = self.run_command(&cmd)?;
        let branches = output
            .lines()
            .map(|line| line.trim_start_matches(|c: char| c == '*' || c == ' ').trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        Ok(branches)
    }

    /// Get commit message for a hash
    pub fn commit_message(&self, commit_hash: &str) -> VcsResult<Option<String>> {
        if commit_hash.trim().is_empty() {
            return Ok(None);
        }

        let cmd = self.get_commit_message_command(commit_hash);
        match self.run_command(&cmd) {
            Ok(output) => Ok(Some(output.trim().to_string())),
            Err(VcsError::CommandFailed { exit_code, .. }) if exit_code != 0 => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get default remote HTTP URL
    pub fn default_remote_http_url(&self) -> VcsResult<Option<String>> {
        let cmd = self.get_default_remote_command();
        match self.run_command(&cmd) {
            Ok(output) => {
                let url = output.trim();
                if url.is_empty() {
                    return Ok(None);
                }
                Ok(Some(self.convert_ssh_to_http(url)))
            }
            Err(VcsError::CommandFailed { exit_code, .. }) if exit_code != 0 => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get the first commit in current branch
    pub fn first_commit_in_current_branch(&self) -> VcsResult<Option<String>> {
        let current_branch = self.current_branch()?;
        let commands = self.get_first_commit_commands(&current_branch);

        for cmd in commands {
            match self.run_command(&cmd) {
                Ok(output) => {
                    let commit = output.trim().to_string();
                    if !commit.is_empty() {
                        return Ok(Some(commit));
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(None)
    }

    /// Get files in a commit
    pub fn files_in_commit(&self, commit_hash: &str) -> VcsResult<Vec<String>> {
        if commit_hash.trim().is_empty() {
            return Ok(vec![]);
        }

        let cmd = self.get_files_in_commit_command(commit_hash);
        match self.run_command(&cmd) {
            Ok(output) => {
                let files = output
                    .lines()
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty())
                    .map(|line| line.replace('\\', "/"))
                    .collect();
                Ok(files)
            }
            Err(VcsError::CommandFailed { exit_code, .. }) if exit_code != 0 => Ok(vec![]),
            Err(e) => Err(e),
        }
    }

    /// Find the most recent commit starting with 'Start-Agent-Branch:'
    pub fn latest_agent_branch_commit(&self) -> VcsResult<String> {
        let cmd = self.get_latest_agent_branch_commit_command();
        let output = self.run_command(&cmd)?;
        Ok(output.trim().to_string())
    }

    /// Setup autopush for a target remote and branch
    pub fn setup_autopush(&self, target_remote_url: &str, target_branch: &str) -> VcsResult<()> {
        let commands = self.get_setup_autopush_commands(target_remote_url, target_branch);
        for cmd in commands {
            self.run_command(&cmd)?;
        }
        Ok(())
    }

    // Private helper methods

    fn find_repo_root<P: AsRef<Path>>(start_path: P) -> VcsResult<PathBuf> {
        let mut current_dir = start_path
            .as_ref()
            .canonicalize()
            .map_err(|_| VcsError::RepositoryNotFound(start_path.as_ref().display().to_string()))?;

        // If the start path is a file, get its parent directory
        if current_dir.is_file() {
            current_dir = current_dir
                .parent()
                .ok_or_else(|| {
                    VcsError::RepositoryNotFound(start_path.as_ref().display().to_string())
                })?
                .to_path_buf();
        }

        loop {
            // Check for VCS directories
            if current_dir.join(".git").exists()
                || current_dir.join(".hg").exists()
                || current_dir.join(".bzr").exists()
                || current_dir.join(".fslckout").exists()
                || current_dir.join("_FOSSIL_").exists()
            {
                return Ok(current_dir);
            }

            // Move up to parent
            let parent = current_dir.parent().ok_or_else(|| {
                VcsError::RepositoryNotFound(start_path.as_ref().display().to_string())
            })?;

            if parent == current_dir {
                break;
            }

            current_dir = parent.to_path_buf();
        }

        Err(VcsError::RepositoryNotFound(
            start_path.as_ref().display().to_string(),
        ))
    }

    fn determine_vcs_type(root_path: &Path) -> VcsResult<VcsType> {
        if root_path.join(".git").exists() {
            Ok(VcsType::Git)
        } else if root_path.join(".hg").exists() {
            Ok(VcsType::Hg)
        } else if root_path.join(".bzr").exists() {
            Ok(VcsType::Bzr)
        } else if root_path.join(".fslckout").exists() || root_path.join("_FOSSIL_").exists() {
            Ok(VcsType::Fossil)
        } else {
            Err(VcsError::VcsTypeNotFound(root_path.display().to_string()))
        }
    }

    fn run_command(&self, cmd: &[String]) -> VcsResult<String> {
        use std::process::Stdio;

        let output = Command::new(&cmd[0])
            .args(&cmd[1..])
            .current_dir(&self.root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("SSH_ASKPASS", "echo")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| VcsError::CommandFailed {
                command: cmd.join(" "),
                exit_code: -1,
                stderr: e.to_string(),
            })?;

        if output.status.success() {
            String::from_utf8(output.stdout).map_err(VcsError::Utf8)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(VcsError::CommandFailed {
                command: cmd.join(" "),
                exit_code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string(),
            })
        }
    }

    fn convert_ssh_to_http(&self, url: &str) -> String {
        // Convert SSH URL to HTTPS format
        if let Some(captures) = Regex::new(r"^git@([^:]+):(.+)$").unwrap().captures(url) {
            // Standard SSH format: git@host:path
            format!("https://{}/{}", &captures[1], &captures[2])
        } else if let Some(captures) =
            Regex::new(r"^ssh://git@([^/]+?)(?::\d+)?/(.+)$").unwrap().captures(url)
        {
            // SSH protocol format: ssh://git@host[:port]/path
            format!("https://{}/{}", &captures[1], &captures[2])
        } else {
            url.to_string()
        }
    }

    // Command builders for each VCS type

    fn get_current_branch_command(&self) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec![
                "git".to_string(),
                "rev-parse".to_string(),
                "--abbrev-ref".to_string(),
                "HEAD".to_string(),
            ],
            VcsType::Hg => vec!["hg".to_string(), "branch".to_string()],
            VcsType::Bzr => vec!["bzr".to_string(), "nick".to_string()],
            VcsType::Fossil => vec![
                "sh".to_string(),
                "-c".to_string(),
                "fossil branch list | grep '*' | sed 's/* //'".to_string(),
            ],
        }
    }

    fn get_start_branch_commands(&self, branch_name: &str) -> Vec<Vec<String>> {
        match self.vcs_type {
            VcsType::Git => vec![vec![
                "git".to_string(),
                "checkout".to_string(),
                "-b".to_string(),
                branch_name.to_string(),
            ]],
            VcsType::Hg => vec![vec![
                "hg".to_string(),
                "branch".to_string(),
                branch_name.to_string(),
            ]],
            VcsType::Bzr => vec![vec![
                "bzr".to_string(),
                "switch".to_string(),
                "-b".to_string(),
                branch_name.to_string(),
            ]],
            VcsType::Fossil => vec![
                vec![
                    "fossil".to_string(),
                    "branch".to_string(),
                    "new".to_string(),
                    branch_name.to_string(),
                    "trunk".to_string(),
                ],
                vec![
                    "fossil".to_string(),
                    "update".to_string(),
                    branch_name.to_string(),
                ],
            ],
        }
    }

    fn get_commit_file_commands(&self, file_path: &str, message: &str) -> Vec<Vec<String>> {
        match self.vcs_type {
            VcsType::Git => vec![
                vec![
                    "git".to_string(),
                    "add".to_string(),
                    "--".to_string(),
                    file_path.to_string(),
                ],
                vec![
                    "git".to_string(),
                    "commit".to_string(),
                    "-m".to_string(),
                    message.to_string(),
                    "--".to_string(),
                    file_path.to_string(),
                ],
            ],
            VcsType::Hg => vec![
                vec!["hg".to_string(), "add".to_string(), file_path.to_string()],
                vec![
                    "hg".to_string(),
                    "commit".to_string(),
                    "-m".to_string(),
                    message.to_string(),
                    file_path.to_string(),
                ],
            ],
            VcsType::Bzr => vec![
                vec!["bzr".to_string(), "add".to_string(), file_path.to_string()],
                vec![
                    "bzr".to_string(),
                    "commit".to_string(),
                    "-m".to_string(),
                    message.to_string(),
                ],
            ],
            VcsType::Fossil => vec![
                vec![
                    "fossil".to_string(),
                    "add".to_string(),
                    file_path.to_string(),
                ],
                vec![
                    "fossil".to_string(),
                    "commit".to_string(),
                    "-m".to_string(),
                    message.to_string(),
                    "--".to_string(),
                    file_path.to_string(),
                ],
            ],
        }
    }

    fn get_push_branch_command(&self, branch_name: &str, remote: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec![
                "git".to_string(),
                "push".to_string(),
                "-u".to_string(),
                remote.to_string(),
                branch_name.to_string(),
            ],
            VcsType::Hg => vec![
                "hg".to_string(),
                "push".to_string(),
                "--new-branch".to_string(),
                "--rev".to_string(),
                branch_name.to_string(),
            ],
            VcsType::Bzr => vec!["bzr".to_string(), "push".to_string()],
            VcsType::Fossil => vec!["fossil".to_string(), "push".to_string()],
        }
    }

    fn get_force_push_branch_command(&self, remote: &str, branch: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec![
                "git".to_string(),
                "push".to_string(),
                remote.to_string(),
                format!("HEAD:{}", branch),
                "--force".to_string(),
            ],
            VcsType::Hg => vec![
                "hg".to_string(),
                "push".to_string(),
                remote.to_string(),
                "--force".to_string(),
                "-b".to_string(),
                branch.to_string(),
            ],
            VcsType::Bzr => vec![
                "bzr".to_string(),
                "push".to_string(),
                remote.to_string(),
                "--overwrite".to_string(),
            ],
            VcsType::Fossil => vec!["fossil".to_string(), "push".to_string(), remote.to_string()],
        }
    }

    fn get_checkout_branch_command(&self, branch_name: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec![
                "git".to_string(),
                "checkout".to_string(),
                branch_name.to_string(),
            ],
            VcsType::Hg => vec![
                "hg".to_string(),
                "update".to_string(),
                branch_name.to_string(),
            ],
            VcsType::Bzr => vec![
                "bzr".to_string(),
                "switch".to_string(),
                branch_name.to_string(),
            ],
            VcsType::Fossil => vec![
                "fossil".to_string(),
                "update".to_string(),
                branch_name.to_string(),
            ],
        }
    }

    fn get_create_local_branch_commands(&self, branch_name: &str) -> VcsResult<Vec<Vec<String>>> {
        match self.vcs_type {
            VcsType::Git => Ok(vec![vec![
                "git".to_string(),
                "checkout".to_string(),
                "-b".to_string(),
                branch_name.to_string(),
            ]]),
            VcsType::Hg => Ok(vec![
                vec![
                    "hg".to_string(),
                    "bookmark".to_string(),
                    branch_name.to_string(),
                ],
                vec![
                    "hg".to_string(),
                    "update".to_string(),
                    branch_name.to_string(),
                ],
            ]),
            VcsType::Fossil => {
                let current_branch = self.current_branch()?;
                Ok(vec![
                    vec![
                        "fossil".to_string(),
                        "branch".to_string(),
                        "new".to_string(),
                        branch_name.to_string(),
                        current_branch,
                    ],
                    vec![
                        "fossil".to_string(),
                        "update".to_string(),
                        branch_name.to_string(),
                    ],
                ])
            }
            VcsType::Bzr => Ok(vec![vec![
                "bzr".to_string(),
                "switch".to_string(),
                "-b".to_string(),
                branch_name.to_string(),
            ]]), // Bzr doesn't have local-only branches
        }
    }

    fn get_add_file_command(&self, file_path: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec!["git".to_string(), "add".to_string(), file_path.to_string()],
            VcsType::Hg => vec!["hg".to_string(), "add".to_string(), file_path.to_string()],
            VcsType::Fossil => vec![
                "fossil".to_string(),
                "add".to_string(),
                file_path.to_string(),
            ],
            VcsType::Bzr => vec!["bzr".to_string(), "add".to_string(), file_path.to_string()],
        }
    }

    fn get_status_command(&self) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec![
                "git".to_string(),
                "status".to_string(),
                "--porcelain".to_string(),
            ],
            VcsType::Hg => vec!["hg".to_string(), "status".to_string()],
            VcsType::Fossil => vec!["fossil".to_string(), "changes".to_string()],
            VcsType::Bzr => vec!["bzr".to_string(), "status".to_string()],
        }
    }

    fn get_tip_commit_command(&self, branch: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec!["git".to_string(), "rev-parse".to_string(), branch.to_string()],
            VcsType::Hg => vec!["hg".to_string(), "log".to_string(), "-r".to_string(), branch.to_string(), "--template".to_string(), "{node}".to_string()],
            VcsType::Fossil => vec![
                "sh".to_string(), "-c".to_string(),
                format!("fossil sql \"SELECT blob.uuid FROM tag JOIN tagxref ON tag.tagid=tagxref.tagid JOIN tagxref ON tag.tagid=tagxref.tagid JOIN blob ON blob.rid=tagxref.rid WHERE tag.tagname='sym-{}' ORDER BY tagxref.mtime DESC LIMIT 1\" | head -n 1", branch)
            ],
            VcsType::Bzr => vec!["bzr".to_string(), "revno".to_string(), "--revision".to_string(), format!("branch:{}", branch)],
        }
    }

    fn get_commit_count_command(&self, base_branch: &str, branch: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec!["git".to_string(), "rev-list".to_string(), format!("{}..{}", base_branch, branch), "--count".to_string()],
            VcsType::Hg => vec![
                "sh".to_string(), "-c".to_string(),
                format!("hg log -r \"branch({}) and not ancestors({})\" --template '{{node}}\\n' | wc -l", branch, base_branch)
            ],
            VcsType::Fossil => vec![
                "sh".to_string(), "-c".to_string(),
                format!("fossil sql \"SELECT count(*) FROM tag JOIN tagxref ON tag.tagid=tagxref.tagid WHERE tag.tagname='sym-{}\'\" | tail -n 1", branch)
            ],
            VcsType::Bzr => vec!["bzr".to_string(), "log".to_string(), "--revision".to_string(), format!("..branch:{}", branch), "--count-only".to_string()],
        }
    }

    fn get_branches_command(&self) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec![
                "git".to_string(),
                "branch".to_string(),
                "--list".to_string(),
            ],
            VcsType::Hg => vec!["hg".to_string(), "branches".to_string()],
            VcsType::Bzr => vec!["bzr".to_string(), "branches".to_string()],
            VcsType::Fossil => vec![
                "fossil".to_string(),
                "branch".to_string(),
                "list".to_string(),
            ],
        }
    }

    fn get_commit_message_command(&self, commit_hash: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec!["git".to_string(), "log".to_string(), "-1".to_string(), "--pretty=format:%B".to_string(), commit_hash.to_string()],
            VcsType::Hg => vec!["hg".to_string(), "log".to_string(), "-r".to_string(), commit_hash.to_string(), "--template".to_string(), "{desc}".to_string()],
            VcsType::Bzr => vec!["bzr".to_string(), "log".to_string(), "-r".to_string(), commit_hash.to_string(), "--show-ids".to_string()],
            VcsType::Fossil => vec![
                "sh".to_string(), "-c".to_string(),
                format!("fossil sql \"SELECT event.comment FROM event JOIN blob ON event.objid=blob.rid WHERE blob.uuid='{}' AND event.type='ci' LIMIT 1\"", commit_hash)
            ],
        }
    }

    fn get_default_remote_command(&self) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec![
                "git".to_string(),
                "remote".to_string(),
                "get-url".to_string(),
                "origin".to_string(),
            ],
            VcsType::Hg => vec!["hg".to_string(), "paths".to_string(), "default".to_string()],
            VcsType::Bzr => vec![
                "bzr".to_string(),
                "config".to_string(),
                "parent_location".to_string(),
            ],
            VcsType::Fossil => vec![
                "sh".to_string(),
                "-c".to_string(),
                "fossil remote | head -n 1".to_string(),
            ],
        }
    }

    fn get_first_commit_commands(&self, current_branch: &str) -> Vec<Vec<String>> {
        match self.vcs_type {
            VcsType::Git => {
                let is_primary_branch = self.is_protected_branch(current_branch);
                if is_primary_branch {
                    vec![vec!["git".to_string(), "rev-list".to_string(), "--max-parents=0".to_string(), "HEAD".to_string(), "--pretty=%H".to_string()]]
                } else {
                    // For feature branches, find merge base and get first commit after it
                    vec![
                        vec!["sh".to_string(), "-c".to_string(), format!("git merge-base {} HEAD", self.default_branch())],
                        vec!["sh".to_string(), "-c".to_string(), "git log --reverse --pretty=%H $1..HEAD | head -n 1".to_string()],
                    ]
                }
            }
            VcsType::Hg => vec![vec!["hg".to_string(), "log".to_string(), "-r".to_string(), format!("min(branch('{}'))", current_branch), "--template".to_string(), "{node}\\n".to_string()]],
            VcsType::Bzr => vec![vec!["bzr".to_string(), "log".to_string(), "-r".to_string(), format!("first(branch('{}'))", current_branch), "--format=rev_id".to_string()]],
            VcsType::Fossil => vec![vec![
                "sh".to_string(), "-c".to_string(),
                format!("fossil sql \"SELECT blob.uuid FROM tag JOIN tagxref ON tag.tagid=tagxref.tagid JOIN event ON tagxref.rid=event.objid JOIN blob ON blob.rid=tagxref.rid WHERE tag.tagname='sym-{}' AND event.comment NOT LIKE 'Create new branch named%' ORDER BY tagxref.mtime ASC LIMIT 1\"", current_branch)
            ]],
        }
    }

    fn get_files_in_commit_command(&self, commit_hash: &str) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec!["git".to_string(), "diff-tree".to_string(), "--no-commit-id".to_string(), "--name-only".to_string(), "-r".to_string(), commit_hash.to_string()],
            VcsType::Hg => vec!["hg".to_string(), "status".to_string(), "--change".to_string(), commit_hash.to_string(), "--no-status".to_string()],
            VcsType::Bzr => vec!["bzr".to_string(), "whatchanged".to_string(), "-r".to_string(), commit_hash.to_string(), "--short".to_string()],
            VcsType::Fossil => vec![
                "sh".to_string(), "-c".to_string(),
                format!("fossil sql \"SELECT filename.name FROM filename JOIN mlink ON filename.fnid=mlink.fnid JOIN mlink ON filename.fnid=mlink.fnid JOIN blob ON mlink.mid=blob.rid WHERE blob.uuid='{}'\"", commit_hash)
            ],
        }
    }

    fn get_latest_agent_branch_commit_command(&self) -> Vec<String> {
        match self.vcs_type {
            VcsType::Git => vec!["git".to_string(), "log".to_string(), "HEAD".to_string(), "-E".to_string(), "--grep=^Start-Agent-Branch:".to_string(), "-n".to_string(), "1".to_string(), "--pretty=%H".to_string()],
            VcsType::Hg => vec!["hg".to_string(), "log".to_string(), "-r".to_string(), "reverse(grep('^Start-Agent-Branch:'))".to_string(), "--limit".to_string(), "1".to_string(), "--template".to_string(), "{node}\\n".to_string()],
            VcsType::Fossil => vec![
                "sh".to_string(), "-c".to_string(),
                "fossil sql \"SELECT blob.uuid FROM event JOIN blob ON event.objid=blob.rid WHERE event.type='ci' AND event.comment LIKE 'Start-Agent-Branch:%' ORDER BY event.mtime DESC LIMIT 1\"".to_string()
            ],
            VcsType::Bzr => vec!["bzr".to_string(), "log".to_string(), "--grep=^Start-Agent-Branch:".to_string(), "--limit=1".to_string(), "--format=commit".to_string()],
        }
    }

    fn get_setup_autopush_commands(
        &self,
        target_remote_url: &str,
        target_branch: &str,
    ) -> Vec<Vec<String>> {
        match self.vcs_type {
            VcsType::Git => vec![
                vec!["git".to_string(), "config".to_string(), "--local".to_string(), "user.name".to_string(), "Agent".to_string()],
                vec!["git".to_string(), "config".to_string(), "--local".to_string(), "user.email".to_string(), "agent@example.com".to_string()],
                vec!["sh".to_string(), "-c".to_string(), format!("git remote add target_remote {} 2>/dev/null || true", target_remote_url)],
                vec!["sh".to_string(), "-c".to_string(), format!("echo '#!/bin/sh\ngit push target_remote HEAD:{} --force' > .git/hooks/post-commit", target_branch)],
                vec!["chmod".to_string(), "755".to_string(), ".git/hooks/post-commit".to_string()],
            ],
            VcsType::Hg => vec![
                vec!["sh".to_string(), "-c".to_string(), "hg log -r . --template '{author}' | head -n 1".to_string()],
                vec!["sh".to_string(), "-c".to_string(), format!("echo '[ui]\nusername = Agent <agent@example.com>\n[paths]\ntarget_remote = {}\n[hooks]\ncommit = hg push target_remote -b {} 2>/dev/null || true' >> .hg/hgrc", target_remote_url, target_branch)],
            ],
            VcsType::Bzr => vec![
                vec!["bzr".to_string(), "whoami".to_string(), "Agent <agent@example.com>".to_string()],
                vec!["sh".to_string(), "-c".to_string(), format!("echo 'push_location = {}' >> .bzr/branch/branch.conf", target_remote_url)],
                vec!["sh".to_string(), "-c".to_string(), "mkdir -p .bzr/hooks && echo '#!/usr/bin/env python\nimport subprocess\nsubprocess.call([\"bzr\", \"push\", \"--quiet\"])' > .bzr/hooks/post_commit.py && chmod 755 .bzr/hooks/post_commit.py".to_string()],
            ],
            VcsType::Fossil => vec![
                vec!["fossil".to_string(), "user".to_string(), "default".to_string(), "Agent".to_string()],
                vec!["sh".to_string(), "-c".to_string(), format!("fossil remote add target_remote {} 2>/dev/null || true", target_remote_url)],
                vec!["fossil".to_string(), "set".to_string(), "autosync".to_string(), "on".to_string()],
            ],
        }
    }
}
