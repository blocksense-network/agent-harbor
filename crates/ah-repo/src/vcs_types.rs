// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Supported VCS types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcsType {
    Git,
    Hg,
    Bzr,
    Fossil,
}

impl VcsType {
    /// Get the default branch name for this VCS type
    pub fn default_branch(&self) -> &'static str {
        match self {
            VcsType::Git => "main",
            VcsType::Hg => "default",
            VcsType::Bzr => "master",
            VcsType::Fossil => "trunk",
        }
    }

    /// Get the protected branch names for this VCS type
    pub fn protected_branches(&self) -> Vec<&'static str> {
        match self {
            VcsType::Git => vec!["main", "master", "trunk", "default"],
            VcsType::Hg => vec!["default"],
            VcsType::Bzr => vec!["master", "trunk"],
            VcsType::Fossil => vec!["trunk"],
        }
    }
}

impl std::fmt::Display for VcsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VcsType::Git => write!(f, "git"),
            VcsType::Hg => write!(f, "hg"),
            VcsType::Bzr => write!(f, "bzr"),
            VcsType::Fossil => write!(f, "fossil"),
        }
    }
}
