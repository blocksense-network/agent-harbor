// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::cmp::min;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::{StreamExt, executor};
use nucleo_matcher::{Matcher, pattern::Pattern};
use tui_textarea::TextArea;

use ah_core::WorkspaceFilesEnumerator;
use ah_repo;
use ah_workflows::WorkspaceWorkflowsEnumerator;
use anyhow::Result;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub const MAX_RESULTS: usize = 50_000; // High ceiling to allow full navigation through large result sets
pub const MENU_WIDTH: u16 = 48;
pub const MAX_MENU_HEIGHT: u16 = 10;
const QUERY_DEBOUNCE: Duration = Duration::from_millis(80);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Trigger {
    Slash,
    At,
}

impl Trigger {
    pub fn as_char(self) -> char {
        match self {
            Trigger::Slash => '/',
            Trigger::At => '@',
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Trigger::Slash => "Workflow",
            Trigger::At => "File",
        }
    }
}

/// Dependencies needed for autocomplete functionality
#[derive(Clone)]
pub struct AutocompleteDependencies {
    pub workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
    pub workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
    pub settings: crate::settings::Settings,
}

impl AutocompleteDependencies {
    /// Create autocomplete dependencies from a VscRepo instance
    pub fn from_vcs_repo(
        vcs_repo: ah_repo::VcsRepo,
        settings: crate::settings::Settings,
    ) -> Result<Self> {
        use ah_workflows::{WorkflowConfig, WorkflowProcessor};

        // Create workspace workflows enumerator first
        let config = WorkflowConfig::default();
        let workflow_processor = WorkflowProcessor::for_repo(config, vcs_repo.root())
            .unwrap_or_else(|_| WorkflowProcessor::new(WorkflowConfig::default()));
        let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> =
            Arc::new(workflow_processor);

        // Create workspace files enumerator from the VcsRepo
        let workspace_files: Arc<dyn WorkspaceFilesEnumerator> = Arc::new(vcs_repo);

        Ok(Self {
            workspace_files,
            workspace_workflows,
            settings,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub trigger: Trigger,
    pub label: String,
    pub detail: Option<String>,
    pub replacement: String,
}

#[derive(Debug, Clone)]
struct InlineMenuViewModel {
    open: bool,
    trigger: Option<Trigger>,
    query: String,
    selected: usize,
    results: Vec<ScoredMatch>,
    token: Option<TokenPosition>,
    last_applied_id: u64,
}

impl Default for InlineMenuViewModel {
    fn default() -> Self {
        Self {
            open: false,
            trigger: None,
            query: String::new(),
            selected: 0,
            results: Vec::new(),
            token: None,
            last_applied_id: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutocompleteKeyResult {
    Consumed { text_changed: bool },
    Ignored,
}

#[derive(Debug, Clone, Copy)]
pub struct AutocompleteMenuState<'a> {
    pub trigger: Trigger,
    pub results: &'a [ScoredMatch],
    pub selected_index: usize,
    pub show_border: bool,
}

pub struct InlineAutocomplete {
    vm: InlineMenuViewModel,
    dependencies: Arc<AutocompleteDependencies>,
    show_border: bool,
    suspended: bool,
    // Caching for fast autocomplete - shared for async access
    // Single Mutex protects entire cache state for thread-safe updates
    pub cache_state: Arc<std::sync::Mutex<CacheState>>,
}

#[derive(Clone, Debug)]
pub struct CacheState {
    pub files: Option<Vec<String>>,
    pub workflows: Option<Vec<String>>,
    pub refresh_in_progress: bool,
    pub last_update: Option<Instant>,
}

impl Clone for InlineAutocomplete {
    fn clone(&self) -> Self {
        Self {
            vm: self.vm.clone(),
            dependencies: Arc::clone(&self.dependencies),
            show_border: self.show_border,
            suspended: self.suspended,
            cache_state: Arc::clone(&self.cache_state),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScoredMatch {
    pub item: Item,
    pub score: u32,
    pub indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TokenPosition {
    row: usize,
    start_col: usize,
}

impl InlineAutocomplete {
    pub fn with_dependencies(dependencies: Arc<AutocompleteDependencies>) -> Self {
        let show_border = dependencies.settings.autocomplete_show_border();
        Self {
            vm: InlineMenuViewModel::default(),
            dependencies,
            show_border,
            suspended: false,
            cache_state: Arc::new(std::sync::Mutex::new(CacheState {
                files: None,
                workflows: None,
                refresh_in_progress: false,
                last_update: None,
            })),
        }
    }

    pub fn set_show_border(&mut self, value: bool) {
        self.show_border = value;
    }

    /// Ensure cache is populated (refresh if needed)
    fn ensure_cache(&mut self) {
        let cache_state = self.cache_state.lock().unwrap();
        let files_cached = cache_state.files.is_some();
        let workflows_cached = cache_state.workflows.is_some();
        if !files_cached || !workflows_cached {
            drop(cache_state); // Release lock before calling start_cache_refresh
            self.start_cache_refresh();
        }
    }

    /// Start asynchronous cache refresh
    fn start_cache_refresh(&mut self) {
        if self.cache_state.lock().unwrap().refresh_in_progress {
            return; // Already refreshing
        }

        // Mark refresh as in progress
        self.cache_state.lock().unwrap().refresh_in_progress = true;

        let dependencies = Arc::clone(&self.dependencies);
        let cache_state = Arc::clone(&self.cache_state);

        tokio::spawn(async move {
            // Refresh files cache
            let files_result = match dependencies.workspace_files.stream_repository_files().await {
                Ok(mut stream) => {
                    let mut files = Vec::new();
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(file) => files.push(file.path),
                            Err(_) => {} // Skip files that can't be read
                        }
                    }
                    Some(files)
                }
                Err(_) => {
                    // If we can't load files, use empty cache
                    Some(Vec::new())
                }
            };

            // Refresh workflows cache
            let workflows_result =
                match dependencies.workspace_workflows.enumerate_workflow_commands().await {
                    Ok(commands) => Some(commands.into_iter().map(|c| c.name).collect()),
                    Err(_) => {
                        // If we can't load workflows, use empty cache
                        Some(Vec::new())
                    }
                };

            // Update cache state using Mutex
            let mut cache_state_mut = cache_state.lock().unwrap();
            cache_state_mut.files = files_result;
            cache_state_mut.workflows = workflows_result;
            cache_state_mut.refresh_in_progress = false;
            cache_state_mut.last_update = Some(Instant::now());
        });
    }

    pub fn notify_text_input(&mut self) {
        self.suspended = false;
    }

    pub fn handle_key_event(
        &mut self,
        key: &KeyEvent,
        textarea: &mut TextArea<'_>,
        needs_redraw: &mut bool,
    ) -> AutocompleteKeyResult {
        if !self.vm.open {
            return AutocompleteKeyResult::Ignored;
        }

        let result = match key.code {
            KeyCode::Up => {
                if !self.vm.results.is_empty() && self.vm.selected > 0 {
                    self.vm.selected -= 1;
                }
                AutocompleteKeyResult::Consumed {
                    text_changed: false,
                }
            }
            KeyCode::Down => {
                if !self.vm.results.is_empty() && self.vm.selected + 1 < self.vm.results.len() {
                    self.vm.selected += 1;
                }
                AutocompleteKeyResult::Consumed {
                    text_changed: false,
                }
            }
            KeyCode::PageUp => {
                if !self.vm.results.is_empty() {
                    let step = min(4, self.vm.results.len().saturating_sub(1));
                    self.vm.selected = self.vm.selected.saturating_sub(step);
                }
                AutocompleteKeyResult::Consumed {
                    text_changed: false,
                }
            }
            KeyCode::PageDown => {
                if !self.vm.results.is_empty() {
                    let step = min(4, self.vm.results.len().saturating_sub(1));
                    self.vm.selected = min(self.vm.selected + step, self.vm.results.len() - 1);
                }
                AutocompleteKeyResult::Consumed {
                    text_changed: false,
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let changed = self.commit_selection(textarea, needs_redraw);
                AutocompleteKeyResult::Consumed {
                    text_changed: changed,
                }
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                let changed = self.commit_selection(textarea, needs_redraw);
                AutocompleteKeyResult::Consumed {
                    text_changed: changed,
                }
            }
            KeyCode::Esc => {
                self.close(needs_redraw);
                self.suspended = true;
                AutocompleteKeyResult::Consumed {
                    text_changed: false,
                }
            }
            _ => AutocompleteKeyResult::Ignored,
        };

        if matches!(result, AutocompleteKeyResult::Consumed { .. }) {
            self.constrain_selected();
        }

        result
    }

    /// Returns true when the menu has open state and at least one result to show
    pub fn is_open(&self) -> bool {
        self.vm.open
    }

    pub fn get_query(&self) -> &str {
        &self.vm.query
    }

    /// Set autocomplete state for testing purposes
    pub fn set_test_state(&mut self, open: bool, query: &str, results: Vec<ScoredMatch>) {
        self.vm.open = open;
        self.vm.query = query.to_string();
        self.vm.results = results;
        // For testing, we need to set the trigger based on the query
        // Assume '/' trigger for testing purposes since our tests use '/'
        self.vm.trigger = Some(Trigger::Slash);
    }

    /// Move the highlighted selection forward, wrapping to the first item.
    /// Returns true when a selection change was attempted (even if only one item exists).
    pub fn select_next(&mut self) -> bool {
        if !self.is_open() {
            return false;
        }

        if self.vm.results.len() > 1 {
            self.vm.selected = (self.vm.selected + 1) % self.vm.results.len();
        }
        self.constrain_selected();
        true
    }

    /// Move the highlighted selection backward, wrapping to the last item.
    /// Returns true when a selection change was attempted (even if only one item exists).
    pub fn select_previous(&mut self) -> bool {
        if !self.is_open() {
            return false;
        }

        if self.vm.results.len() > 1 {
            if self.vm.selected == 0 {
                self.vm.selected = self.vm.results.len() - 1;
            } else {
                self.vm.selected -= 1;
            }
        }
        self.constrain_selected();
        true
    }

    pub fn after_textarea_change(&mut self, textarea: &TextArea<'_>, needs_redraw: &mut bool) {
        if self.suspended {
            return;
        }
        if let Some((trigger, token, query)) = extract_token(textarea) {
            let same_trigger = self.vm.trigger == Some(trigger);

            if same_trigger {
                // Same trigger – check if query changed and perform search if needed
                let was_open = self.vm.open;
                self.vm.token = Some(token);
                let query_changed = self.vm.query != query;
                self.vm.query = query.clone(); // Update query as cursor moves

                if query_changed {
                    // Query changed - perform search with new query
                    self.perform_search(trigger, query);
                } else {
                    // Same query - just keep menu visible
                    self.vm.open = self.vm.open || !self.vm.results.is_empty();
                }

                if was_open != self.vm.open || query_changed {
                    *needs_redraw = true;
                }
            } else {
                // New trigger context - reset selection and perform search
                self.vm.selected = 0;
                self.vm.trigger = Some(trigger);
                self.vm.token = Some(token);
                self.vm.query = query.clone();
                self.vm.open = false;

                // Perform search directly using enumerators
                self.perform_search(trigger, query);
                *needs_redraw = true;
            }
        } else {
            self.close(needs_redraw);
        }
    }

    fn perform_search(&mut self, trigger: Trigger, needle: String) {
        // Hide menu if query is empty
        if needle.is_empty() {
            self.close(&mut false);
            return;
        }

        // Ensure cache is populated
        self.ensure_cache();

        let candidates = {
            let cache_state = self.cache_state.lock().unwrap();
            match trigger {
                Trigger::At => cache_state.files.as_ref().unwrap_or(&vec![]).clone(),
                Trigger::Slash => cache_state.workflows.as_ref().unwrap_or(&vec![]).clone(),
            }
        };

        // Perform fuzzy matching
        let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
        let pattern = Pattern::new(
            needle.as_str(),
            nucleo_matcher::pattern::CaseMatching::Ignore,
            nucleo_matcher::pattern::Normalization::Smart,
            nucleo_matcher::pattern::AtomKind::Fuzzy,
        );

        let mut results: Vec<ScoredMatch> = candidates
            .iter()
            .filter_map(|candidate| {
                let haystack = nucleo_matcher::Utf32String::from(candidate.as_str());
                let score = pattern.score(haystack.slice(..), &mut matcher);
                if score.is_some() {
                    Some(ScoredMatch {
                        item: Item {
                            id: candidate.clone(),
                            trigger,
                            label: candidate.clone(),
                            detail: None, // Could add more details later
                            replacement: format!("{}{}", trigger.as_char(), candidate),
                        },
                        score: score.unwrap_or(0),
                        indices: Vec::new(), // Indices not supported in this version
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (higher is better)
        results.sort_by(|a, b| b.score.cmp(&a.score));

        // Limit results
        results.truncate(MAX_RESULTS);

        // Update results
        self.vm.results = results;
        self.vm.last_applied_id += 1;

        // Update selection bounds
        if self.vm.selected >= self.vm.results.len() {
            self.vm.selected = self.vm.results.len().saturating_sub(1);
        }

        // Show menu if we have results, hide if empty
        if self.vm.results.is_empty() {
            self.close(&mut false);
        } else {
            self.vm.open = true;
        }
    }

    pub fn menu_state(&self) -> Option<AutocompleteMenuState<'_>> {
        if !self.vm.open {
            return None;
        }

        let trigger = self.vm.trigger?;
        if self.vm.results.is_empty() {
            return None;
        }

        let len = self.vm.results.len();
        let selected = self.vm.selected.min(len.saturating_sub(1));

        Some(AutocompleteMenuState {
            trigger,
            results: &self.vm.results,
            selected_index: selected,
            show_border: self.show_border,
        })
    }

    fn commit_selection(&mut self, textarea: &mut TextArea<'_>, needs_redraw: &mut bool) -> bool {
        if self.vm.results.is_empty() {
            self.close(needs_redraw);
            return false;
        }

        let mut changed = false;

        if let Some(ref token) = self.vm.token {
            if let Some(result) = self.vm.results.get(self.vm.selected) {
                if textarea.cursor().0 == token.row {
                    let current_col = textarea.cursor().1;
                    if current_col >= token.start_col {
                        let steps_back = current_col - token.start_col;
                        for _ in 0..steps_back {
                            textarea.move_cursor(tui_textarea::CursorMove::Back);
                        }
                        textarea.start_selection();
                        for _ in 0..steps_back {
                            textarea.move_cursor(tui_textarea::CursorMove::Forward);
                        }
                        if textarea.cut() {
                            changed = true;
                        }
                        for ch in result.item.replacement.chars() {
                            textarea.insert_char(ch);
                            changed = true;
                        }
                    }
                }
            }
        }

        self.close(needs_redraw);
        changed
    }

    /// Close the autocomplete menu and clear any pending state.
    pub fn close(&mut self, needs_redraw: &mut bool) {
        let was_open = self.vm.open;
        self.vm.open = false;
        self.vm.trigger = None;
        self.vm.results.clear();
        self.vm.selected = 0;
        self.vm.token = None;
        if was_open {
            *needs_redraw = true;
        }
    }

    fn constrain_selected(&mut self) {
        if self.vm.results.is_empty() {
            self.vm.selected = 0;
            return;
        }
        if self.vm.selected >= self.vm.results.len() {
            self.vm.selected = self.vm.results.len().saturating_sub(1);
        }
    }
}

fn extract_token(textarea: &TextArea<'_>) -> Option<(Trigger, TokenPosition, String)> {
    let (row, col) = textarea.cursor();
    let line = textarea.lines().get(row)?;

    let mut current_char = 0usize;
    let mut cursor_byte = line.len();
    for (byte_idx, _ch) in line.char_indices() {
        if current_char == col {
            cursor_byte = byte_idx;
            break;
        }
        current_char += 1;
    }
    if current_char == col {
        cursor_byte = cursor_byte.min(line.len());
    }

    let prefix = &line[..cursor_byte];
    let positions: Vec<(usize, usize, char)> = prefix
        .char_indices()
        .enumerate()
        .map(|(char_idx, (byte_idx, ch))| (char_idx, byte_idx, ch))
        .collect();

    let mut trigger_idx = None;
    for &(char_idx, byte_idx, ch) in positions.iter().rev() {
        if ch == '@' {
            trigger_idx = Some((char_idx, byte_idx, Trigger::At));
            break;
        }
        if ch == '/' {
            trigger_idx = Some((char_idx, byte_idx, Trigger::Slash));
            break;
        }
        if is_token_boundary(ch) {
            return None;
        }
    }

    let (char_idx, byte_idx, trigger) = trigger_idx?;
    if char_idx > 0 {
        let (_, _, prev) = positions[char_idx - 1];
        if !is_token_boundary(prev) {
            return None;
        }
    }

    let query = prefix[byte_idx + trigger.as_char().len_utf8()..].to_string();
    Some((
        trigger,
        TokenPosition {
            row,
            start_col: char_idx,
        },
        query,
    ))
}

fn is_token_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' | '\'' | '"' | '`'
        )
}
