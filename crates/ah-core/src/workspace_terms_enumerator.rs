// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//
//! Workspace term enumeration and prefix search support.
//!
//! This module provides a background indexer that walks the tracked files in a
//! repository and maintains a compressed prefix-searchable vocabulary. The
//! resulting index can be queried synchronously from the TUI to provide fast
//! autocomplete suggestions without re-reading the filesystem on every key
//! stroke.
//!
//! The implementation favours read-heavy workloads: once the index is built,
//! lookups are lock-free except for a short read guard, and the search cost is
//! linear in the length of the query string plus the number of returned
//! results. Index refreshes happen on background Tokio tasks so they never
//! block the UI thread.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use fst::{IntoStreamer, Streamer, automaton::Automaton};
use futures::StreamExt;
use thiserror::Error;
use tokio::runtime::{Builder as TokioBuilder, Handle};

use crate::workspace_files_enumerator::{
    RepositoryError, RepositoryFile, WorkspaceFilesEnumerator,
};

/// Default upper bound on the number of unique terms retained in memory.
///
/// Large repositories can easily expose hundreds of thousands of unique
/// identifiers; we cap the in-memory index to keep the footprint reasonable
/// while still giving high quality suggestions.
const DEFAULT_MAX_TERMS: usize = 150_000;

/// Autocomplete term returned by the enumerator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermEntry {
    /// The completion candidate.
    pub term: String,
    /// Relative weight of the term (higher is better).
    pub weight: u32,
}

/// Detailed lookup information used by interactive clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupOutcome {
    /// Characters that every match shares beyond the queried prefix.
    ///
    /// This is the portion that can be confidently inserted with a single TAB.
    pub shared_extension: String,
    /// Characters required to reach the shortest matching candidate.
    ///
    /// When this differs from `shared_extension`, it represents a second TAB
    /// completion step. When the lookup yields a single match the two strings
    /// are identical.
    pub shortest_completion: String,
    /// Raw ranked matches for menu display and scoring.
    pub entries: Vec<TermEntry>,
}

impl LookupOutcome {
    pub fn empty() -> Self {
        Self {
            shared_extension: String::new(),
            shortest_completion: String::new(),
            entries: Vec::new(),
        }
    }
}

/// Result type for workspace term enumeration.
pub type TermsResult<T> = Result<T, WorkspaceTermsError>;

/// Errors emitted when building or querying the workspace term index.
#[derive(Error, Debug)]
pub enum WorkspaceTermsError {
    #[error("failed to enumerate repository files: {0}")]
    Repository(#[from] crate::workspace_files_enumerator::RepositoryError),
    #[error("failed to build term index: {0}")]
    Index(#[from] fst::Error),
}

/// Trait implemented by workspace term providers.
pub trait WorkspaceTermsEnumerator: Send + Sync {
    /// Return completion candidates for the provided prefix.
    ///
    /// Implementations should return results ordered by their internal scoring
    /// (usually lexicographic order or frequency) and never more than `limit`
    /// entries. The returned [`LookupOutcome`] also contains aggregate metadata
    /// required for inline ghost completions.
    fn lookup(&self, prefix: &str, limit: usize) -> LookupOutcome;

    /// Signal the enumerator to refresh its internal index in the background.
    fn request_refresh(&self);

    /// Whether the index has been built at least once.
    fn is_ready(&self) -> bool;

    /// Timestamp of the most recent successful refresh, if any.
    fn last_updated(&self) -> Option<Instant>;
}

#[derive(Debug, Clone)]
struct TermsIndex {
    fst: Arc<fst::Set<Vec<u8>>>,
}

impl TermsIndex {
    fn new(terms: Vec<String>) -> TermsResult<Self> {
        let fst = fst::Set::from_iter(terms.iter())?;
        Ok(Self { fst: Arc::new(fst) })
    }

    fn lookup(&self, prefix: &str, limit: usize) -> Vec<String> {
        let limit = limit.max(1);

        // Optimise empty prefix: we simply stream from the start of the set.
        if prefix.is_empty() {
            return collect_strings(self.fst.stream(), limit);
        }

        let automaton = fst::automaton::Str::new(prefix).starts_with();
        collect_strings(self.fst.search(automaton).into_stream(), limit)
    }
}

fn collect_strings<A>(mut stream: fst::set::Stream<'_, A>, limit: usize) -> Vec<String>
where
    A: Automaton,
{
    let mut results = Vec::new();
    while results.len() < limit {
        match stream.next() {
            Some(bytes) => {
                if let Ok(text) = std::str::from_utf8(bytes) {
                    results.push(text.to_string());
                }
            }
            None => break,
        }
    }
    results
}

#[derive(Debug)]
struct TermsState {
    index: Option<TermsIndex>,
    building: bool,
    last_updated: Option<Instant>,
    last_error: Option<WorkspaceTermsError>,
}

impl Default for TermsState {
    fn default() -> Self {
        Self {
            index: None,
            building: false,
            last_updated: None,
            last_error: None,
        }
    }
}

/// Default implementation backed by `ah_repo::VcsRepo`.
pub struct DefaultWorkspaceTermsEnumerator {
    files_enumerator: Arc<dyn WorkspaceFilesEnumerator>,
    max_terms: usize,
    state: Arc<RwLock<TermsState>>,
}

impl DefaultWorkspaceTermsEnumerator {
    /// Create a new enumerator and immediately start indexing in the background.
    pub fn new(files_enumerator: Arc<dyn WorkspaceFilesEnumerator>) -> Self {
        let enumerator = Self {
            files_enumerator,
            max_terms: DEFAULT_MAX_TERMS,
            state: Arc::new(RwLock::new(TermsState::default())),
        };
        enumerator.spawn_refresh();
        enumerator
    }

    /// Force a background refresh using the current repository handle.
    fn spawn_refresh(&self) {
        {
            let mut state = self.state.write().expect("lock poisoned");
            if state.building {
                return;
            }
            state.building = true;
        }

        let files_enumerator = Arc::clone(&self.files_enumerator);
        let state_for_future = Arc::clone(&self.state);
        let state_for_error = Arc::clone(&self.state);
        let max_terms = self.max_terms;

        let indexing_future = async move {
            let result = build_index(files_enumerator, max_terms).await;
            let mut guard = state_for_future.write().expect("lock poisoned");
            match result {
                Ok(index) => {
                    guard.last_error = None;
                    guard.last_updated = Some(Instant::now());
                    guard.index = Some(index);
                }
                Err(err) => {
                    guard.last_error = Some(err);
                }
            }
            guard.building = false;
        };

        if let Ok(handle) = Handle::try_current() {
            handle.spawn(indexing_future);
        } else {
            std::thread::spawn(move || {
                if let Ok(runtime) = TokioBuilder::new_current_thread().enable_all().build() {
                    runtime.block_on(indexing_future);
                } else {
                    let mut guard = state_for_error.write().expect("lock poisoned");
                    guard.building = false;
                    guard.last_error = Some(WorkspaceTermsError::Repository(
                        RepositoryError::Other("failed to create Tokio runtime".to_string()),
                    ));
                }
            });
        }
    }

    /// Block the current thread until the index has been built or the timeout elapses.
    pub fn wait_until_ready(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            {
                let guard = self.state.read().expect("lock poisoned");
                if guard.index.is_some() {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }
}

async fn build_index(
    files_enumerator: Arc<dyn WorkspaceFilesEnumerator>,
    max_terms: usize,
) -> TermsResult<TermsIndex> {
    let mut stream = files_enumerator.stream_repository_files().await?;
    let mut vocabulary: BTreeSet<String> = BTreeSet::new();

    while let Some(item) = stream.next().await {
        let path = match item {
            Ok(RepositoryFile { path, .. }) => path,
            Err(_) => continue,
        };

        insert_terms_for_path(&mut vocabulary, &path);
        if vocabulary.len() >= max_terms {
            break;
        }
    }

    let terms: Vec<String> = vocabulary.into_iter().collect();
    TermsIndex::new(terms)
}

fn insert_terms_for_path(set: &mut BTreeSet<String>, path: &str) {
    let normalized = path.replace('\\', "/");

    if !normalized.is_empty() {
        set.insert(normalized.clone());
    }

    let path_buf = PathBuf::from(&normalized);
    if let Some(name) = path_buf.file_name() {
        let name = name.to_string_lossy().to_string();
        if !name.is_empty() {
            set.insert(name.clone());
        }

        if let Some(stem) = Path::new(&name).file_stem() {
            let stem = stem.to_string_lossy().to_string();
            if stem.len() >= 3 {
                set.insert(stem);
            }
        }
    }

    for component in path_buf.components() {
        if let std::path::Component::Normal(seg) = component {
            let segment = seg.to_string_lossy().to_string();
            if !segment.is_empty() {
                set.insert(segment);
            }
        }
    }
}

fn compute_lookup_metadata(prefix: &str, terms: &[String]) -> (String, String) {
    if terms.is_empty() {
        return (String::new(), String::new());
    }

    let prefix_len = prefix.chars().count();
    let mut shared = terms[0].clone();
    let mut shortest = terms[0].clone();

    for term in terms.iter().skip(1) {
        if term.len() < shortest.len() {
            shortest = term.clone();
        }
        let common_prefix: String = shared
            .chars()
            .zip(term.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a)
            .collect();
        shared = common_prefix;
        if shared.chars().count() == prefix_len {
            break;
        }
    }

    if shared.chars().count() < prefix_len {
        shared = prefix.to_string();
    }

    let shared_extension: String = shared.chars().skip(prefix_len).collect();
    let shortest_completion: String = if shortest.chars().count() > prefix_len {
        shortest.chars().skip(prefix_len).collect()
    } else {
        String::new()
    };

    (shared_extension, shortest_completion)
}

impl WorkspaceTermsEnumerator for DefaultWorkspaceTermsEnumerator {
    fn lookup(&self, prefix: &str, limit: usize) -> LookupOutcome {
        let guard = self.state.read().expect("lock poisoned");
        if let Some(index) = guard.index.as_ref() {
            let raw_terms = index.lookup(prefix, limit);
            let (shared_extension, shortest_completion) =
                compute_lookup_metadata(prefix, &raw_terms);
            let entries = raw_terms.into_iter().map(|term| TermEntry { term, weight: 0 }).collect();
            LookupOutcome {
                shared_extension,
                shortest_completion,
                entries,
            }
        } else {
            LookupOutcome::empty()
        }
    }

    fn request_refresh(&self) {
        self.spawn_refresh();
    }

    fn is_ready(&self) -> bool {
        let guard = self.state.read().expect("lock poisoned");
        guard.index.is_some()
    }

    fn last_updated(&self) -> Option<Instant> {
        let guard = self.state.read().expect("lock poisoned");
        guard.last_updated
    }
}

/// Simple in-memory enumerator suitable for deterministic tests.
#[derive(Clone, Default)]
pub struct MockWorkspaceTermsEnumerator {
    terms: Vec<String>,
}

impl MockWorkspaceTermsEnumerator {
    pub fn new<T: Into<String>>(terms: impl IntoIterator<Item = T>) -> Self {
        Self {
            terms: terms.into_iter().map(|t| t.into()).collect(),
        }
    }
}

impl WorkspaceTermsEnumerator for MockWorkspaceTermsEnumerator {
    fn lookup(&self, prefix: &str, limit: usize) -> LookupOutcome {
        let mut filtered: Vec<String> = self
            .terms
            .iter()
            .filter(|term| term.to_lowercase().starts_with(&prefix.to_lowercase()))
            .take(limit)
            .cloned()
            .collect();

        if filtered.is_empty() {
            return LookupOutcome::empty();
        }

        filtered.sort();
        let (shared_extension, shortest_completion) = compute_lookup_metadata(prefix, &filtered);
        let entries = filtered.into_iter().map(|term| TermEntry { term, weight: 0 }).collect();

        LookupOutcome {
            shared_extension,
            shortest_completion,
            entries,
        }
    }

    fn request_refresh(&self) {}

    fn is_ready(&self) -> bool {
        true
    }

    fn last_updated(&self) -> Option<Instant> {
        Some(Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_files_enumerator::MockWorkspaceFilesEnumerator;
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn builds_index_and_returns_prefix_matches() {
        let files = vec![
            RepositoryFile {
                path: "src/lib.rs".to_string(),
                detail: None,
            },
            RepositoryFile {
                path: "src/main.rs".to_string(),
                detail: None,
            },
            RepositoryFile {
                path: "README.md".to_string(),
                detail: None,
            },
        ];
        let files_enumerator: Arc<dyn WorkspaceFilesEnumerator> = Arc::new(
            MockWorkspaceFilesEnumerator::new(files).with_delay(Duration::from_millis(10)),
        );
        let enumerator = DefaultWorkspaceTermsEnumerator::new(files_enumerator);

        let start = Instant::now();
        while !enumerator.is_ready() && start.elapsed() < Duration::from_secs(2) {
            sleep(Duration::from_millis(10)).await;
        }

        assert!(enumerator.is_ready());

        let outcome = enumerator.lookup("src/", 10);
        assert!(outcome.entries.iter().any(|entry| entry.term == "src/lib.rs"));
        assert!(outcome.entries.iter().any(|entry| entry.term == "src/main.rs"));
        assert_eq!(outcome.shared_extension, "");

        let outcome = enumerator.lookup("READ", 5);
        assert!(outcome.entries.iter().any(|entry| entry.term == "README.md"));
        assert_eq!(outcome.shared_extension, "ME");
        assert_eq!(outcome.shortest_completion, "ME");
    }
}
