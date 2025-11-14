// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::cmp::{Ordering, min};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use futures::{StreamExt, executor};
use nucleo_matcher::{Matcher, pattern::Pattern};
use tui_textarea::TextArea;

use ah_core::{
    DefaultWorkspaceTermsEnumerator, LookupOutcome, TermEntry, WorkspaceFilesEnumerator,
    WorkspaceTermsEnumerator,
};
use ah_repo::VcsRepo;
use ah_workflows::WorkspaceWorkflowsEnumerator;
use anyhow::Result;

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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MenuContext {
    Trigger(Trigger),
    WorkspaceTerms,
}

impl MenuContext {
    pub fn title(self) -> &'static str {
        match self {
            MenuContext::Trigger(trigger) => trigger.display_label(),
            MenuContext::WorkspaceTerms => "Workspace Terms",
        }
    }

    pub fn leading_symbol(self) -> Option<char> {
        match self {
            MenuContext::Trigger(trigger) => Some(trigger.as_char()),
            MenuContext::WorkspaceTerms => None,
        }
    }
}

/// Dependencies needed for autocomplete functionality
#[derive(Clone)]
pub struct AutocompleteDependencies {
    pub workspace_files: Arc<dyn WorkspaceFilesEnumerator>,
    pub workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator>,
    pub workspace_terms: Arc<dyn WorkspaceTermsEnumerator>,
    pub settings: crate::settings::Settings,
}

impl AutocompleteDependencies {
    /// Create autocomplete dependencies from a VscRepo instance
    pub fn from_vcs_repo(vcs_repo: VcsRepo, settings: crate::settings::Settings) -> Result<Self> {
        use ah_workflows::{WorkflowConfig, WorkflowProcessor};

        let repo = Arc::new(vcs_repo);

        // Create workspace workflows enumerator first
        let config = WorkflowConfig::default();
        let workflow_processor = WorkflowProcessor::for_repo(config, repo.root())
            .unwrap_or_else(|_| WorkflowProcessor::new(WorkflowConfig::default()));
        let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> =
            Arc::new(workflow_processor);

        // Create workspace files enumerator from the VcsRepo
        let workspace_files: Arc<dyn WorkspaceFilesEnumerator> = repo.clone();

        // Create workspace terms enumerator (background indexing)
        let workspace_terms: Arc<dyn WorkspaceTermsEnumerator> = Arc::new(
            DefaultWorkspaceTermsEnumerator::new(Arc::clone(&workspace_files)),
        );

        Ok(Self {
            workspace_files,
            workspace_workflows,
            workspace_terms,
            settings,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub context: MenuContext,
    pub label: String,
    pub detail: Option<String>,
    pub replacement: String,
}

#[derive(Debug, Clone)]
struct InlineMenuViewModel {
    open: bool,
    context: Option<MenuContext>,
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
            context: None,
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
    pub context: MenuContext,
    pub results: &'a [ScoredMatch],
    pub selected_index: usize,
    pub show_border: bool,
}

pub struct InlineAutocomplete {
    vm: InlineMenuViewModel,
    dependencies: Arc<AutocompleteDependencies>,
    show_border: bool,
    terms_menu_enabled: bool,
    suspended: bool,
    // Caching for fast autocomplete - shared for async access
    // Single Mutex protects entire cache state for thread-safe updates
    pub cache_state: Arc<std::sync::Mutex<CacheState>>,
    ghost: Option<GhostState>,
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
            terms_menu_enabled: self.terms_menu_enabled,
            suspended: self.suspended,
            cache_state: Arc::clone(&self.cache_state),
            ghost: self.ghost.clone(),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GhostSource {
    Menu,
    Terms,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhostState {
    token: TokenPosition,
    typed_len: usize,
    shared_extension: String,
    completion_extension: String,
    pub source: GhostSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutocompleteAcceptance {
    SharedExtension,
    FullCompletion,
}

impl GhostState {
    pub fn row(&self) -> usize {
        self.token.row
    }

    pub fn start_col(&self) -> usize {
        self.token.start_col
    }

    pub fn typed_len(&self) -> usize {
        self.typed_len
    }

    pub fn shared_extension(&self) -> &str {
        &self.shared_extension
    }

    pub fn completion_extension(&self) -> &str {
        &self.completion_extension
    }

    pub fn extra_completion(&self) -> &str {
        if self.completion_extension.len() <= self.shared_extension.len() {
            ""
        } else {
            &self.completion_extension[self.shared_extension.len()..]
        }
    }

    pub fn is_empty(&self) -> bool {
        self.shared_extension.is_empty() && self.completion_extension.is_empty()
    }

    pub fn clear_shared_extension(&mut self) {
        self.shared_extension.clear();
    }
}

impl InlineAutocomplete {
    pub fn with_dependencies(dependencies: Arc<AutocompleteDependencies>) -> Self {
        let show_border = dependencies.settings.autocomplete_show_border();
        let terms_menu_enabled = dependencies.settings.workspace_terms_menu();
        Self {
            vm: InlineMenuViewModel::default(),
            dependencies,
            show_border,
            terms_menu_enabled,
            suspended: false,
            cache_state: Arc::new(std::sync::Mutex::new(CacheState {
                files: None,
                workflows: None,
                refresh_in_progress: false,
                last_update: None,
            })),
            ghost: None,
        }
    }

    pub fn set_show_border(&mut self, value: bool) {
        self.show_border = value;
    }

    /// Ensure cache is populated (refresh if needed)
    fn ensure_cache(&mut self) {
        let (need_files, need_workflows) = {
            let cache_state = self.cache_state.lock().unwrap();
            (cache_state.files.is_none(), cache_state.workflows.is_none())
        };

        if need_files {
            if let Ok(mut stream) =
                executor::block_on(self.dependencies.workspace_files.stream_repository_files())
            {
                let files = executor::block_on(async {
                    let mut collected = Vec::new();
                    while let Some(item) = stream.next().await {
                        if let Ok(repo_file) = item {
                            collected.push(repo_file.path);
                        }
                        if collected.len() >= MAX_RESULTS {
                            break;
                        }
                    }
                    collected
                });
                let mut cache_state = self.cache_state.lock().unwrap();
                if cache_state.files.is_none() {
                    cache_state.files = Some(files);
                }
            }
        }

        if need_workflows {
            if let Ok(commands) = executor::block_on(
                self.dependencies.workspace_workflows.enumerate_workflow_commands(),
            ) {
                let workflows: Vec<String> = commands.into_iter().map(|c| c.name).collect();
                let mut cache_state = self.cache_state.lock().unwrap();
                if cache_state.workflows.is_none() {
                    cache_state.workflows = Some(workflows);
                }
            }
        }

        self.start_cache_refresh();
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

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                // Refresh files cache
                let files_result =
                    match dependencies.workspace_files.stream_repository_files().await {
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
        } else {
            // No async runtime available (e.g., synchronous unit tests). Mark refresh as finished.
            let mut cache_state_mut = self.cache_state.lock().unwrap();
            cache_state_mut.refresh_in_progress = false;
            cache_state_mut.last_update = Some(Instant::now());
        }
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
            if key.code == KeyCode::Tab && key.modifiers.is_empty() {
                let changed = self.accept_ghost(textarea, needs_redraw, false);
                if changed {
                    return AutocompleteKeyResult::Consumed { text_changed: true };
                }
            }
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

    pub fn ghost_state(&self) -> Option<&GhostState> {
        self.ghost.as_ref()
    }

    pub fn has_actionable_suggestion(&self) -> bool {
        self.is_open() || self.ghost.is_some()
    }

    pub fn accept_completion(
        &mut self,
        textarea: &mut TextArea<'_>,
        needs_redraw: &mut bool,
        acceptance: AutocompleteAcceptance,
    ) -> bool {
        if self.is_open() {
            return self.commit_current_selection(textarea, needs_redraw);
        }

        let full_completion = matches!(acceptance, AutocompleteAcceptance::FullCompletion);
        self.accept_ghost(textarea, needs_redraw, full_completion)
    }

    /// Set autocomplete state for testing purposes
    pub fn set_test_state(&mut self, open: bool, query: &str, results: Vec<ScoredMatch>) {
        self.vm.open = open;
        self.vm.query = query.to_string();
        self.vm.results = results;
        // For testing, we need to set the trigger based on the query
        // Assume '/' trigger for testing purposes since our tests use '/'
        self.vm.context = Some(MenuContext::Trigger(Trigger::Slash));
    }

    fn set_ghost_state(&mut self, ghost: Option<GhostState>) -> bool {
        if self.ghost != ghost {
            self.ghost = ghost;
            true
        } else {
            false
        }
    }

    /// Move the highlighted selection forward, wrapping to the first item.
    /// Returns true when a selection change was attempted (even if only one item exists).
    pub fn select_next(&mut self) -> bool {
        if !self.is_open() || self.vm.results.is_empty() {
            return false;
        }

        let previous = self.vm.selected;
        if self.vm.results.len() > 1 {
            self.vm.selected = (self.vm.selected + 1) % self.vm.results.len();
        }
        self.constrain_selected();
        let changed = previous != self.vm.selected;
        if changed {
            self.update_menu_ghost();
        }
        changed
    }

    /// Move the highlighted selection backward, wrapping to the last item.
    /// Returns true when a selection change was attempted (even if only one item exists).
    pub fn select_previous(&mut self) -> bool {
        if !self.is_open() || self.vm.results.is_empty() {
            return false;
        }

        let previous = self.vm.selected;
        if self.vm.results.len() > 1 {
            if self.vm.selected == 0 {
                self.vm.selected = self.vm.results.len() - 1;
            } else {
                self.vm.selected -= 1;
            }
        }
        self.constrain_selected();
        let changed = previous != self.vm.selected;
        if changed {
            self.update_menu_ghost();
        }
        changed
    }

    pub fn set_selected_index(&mut self, index: usize) -> bool {
        if !self.is_open() || self.vm.results.is_empty() {
            return false;
        }

        let target = index.min(self.vm.results.len().saturating_sub(1));
        if self.vm.selected != target {
            self.vm.selected = target;
            self.update_menu_ghost();
            true
        } else {
            false
        }
    }

    pub fn commit_current_selection(
        &mut self,
        textarea: &mut TextArea<'_>,
        needs_redraw: &mut bool,
    ) -> bool {
        if !self.is_open() || self.vm.results.is_empty() {
            return false;
        }

        let changed = self.commit_selection(textarea, needs_redraw);
        if changed {
            self.set_ghost_state(None);
        }
        changed
    }

    pub fn accept_ghost(
        &mut self,
        textarea: &mut TextArea<'_>,
        needs_redraw: &mut bool,
        full_completion: bool,
    ) -> bool {
        let ghost = match self.ghost.clone() {
            Some(ghost) => ghost,
            None => return false,
        };

        match ghost.source {
            GhostSource::Menu => self.commit_current_selection(textarea, needs_redraw),
            GhostSource::Terms => {
                let extension = if full_completion {
                    // Full completion: insert the full shortest completion
                    ghost.completion_extension().to_string()
                } else {
                    // Two-step completion for TAB
                    if !ghost.shared_extension().is_empty() {
                        // First TAB: insert shared extension
                        let ext = ghost.shared_extension().to_string();
                        // After inserting, update ghost state to clear shared_extension
                        // so next TAB will insert extra completion
                        if let Some(ref mut current_ghost) = self.ghost {
                            if let GhostSource::Terms = current_ghost.source {
                                current_ghost.clear_shared_extension();
                            }
                        }
                        ext
                    } else {
                        // Second TAB: insert extra completion
                        ghost.extra_completion().to_string()
                    }
                };

                if extension.is_empty() {
                    return false;
                }

                for ch in extension.chars() {
                    textarea.insert_char(ch);
                }
                *needs_redraw = true;
                self.after_textarea_change(textarea, needs_redraw);
                true
            }
        }
    }

    fn update_menu_ghost(&mut self) -> bool {
        if !self.is_open() || self.vm.results.is_empty() {
            return self.set_ghost_state(None);
        }

        let context = match self.vm.context {
            Some(context) => context,
            None => return self.set_ghost_state(None),
        };
        let token = match self.vm.token.clone() {
            Some(token) => token,
            None => return self.set_ghost_state(None),
        };

        let selected = self.vm.selected.min(self.vm.results.len().saturating_sub(1));
        let item = match self.vm.results.get(selected) {
            Some(item) => item,
            None => return self.set_ghost_state(None),
        };

        let typed_prefix = match context {
            MenuContext::Trigger(trigger) => {
                let mut prefix = String::new();
                prefix.push(trigger.as_char());
                prefix.push_str(&self.vm.query);
                prefix
            }
            MenuContext::WorkspaceTerms => self.vm.query.clone(),
        };
        if let Some(remainder) = compute_remainder(&item.item.replacement, &typed_prefix) {
            if remainder.is_empty() {
                return self.set_ghost_state(None);
            }
            let ghost = GhostState {
                token,
                typed_len: typed_prefix.chars().count(),
                shared_extension: remainder.clone(),
                completion_extension: remainder,
                source: GhostSource::Menu,
            };
            self.set_ghost_state(Some(ghost))
        } else {
            self.set_ghost_state(None)
        }
    }

    fn clear_terms_menu(&mut self) {
        if matches!(self.vm.context, Some(MenuContext::WorkspaceTerms)) {
            self.vm.open = false;
            self.vm.context = None;
            self.vm.results.clear();
            self.vm.selected = 0;
            self.vm.token = None;
            self.vm.query.clear();
        }
    }

    fn open_terms_menu(
        &mut self,
        token: TokenPosition,
        prefix: String,
        entries: &[TermEntry],
    ) -> bool {
        if !self.terms_menu_enabled || entries.is_empty() {
            self.clear_terms_menu();
            return false;
        }

        let previous_selected_id =
            self.vm.results.get(self.vm.selected).map(|item| item.item.id.clone());

        let mut results: Vec<ScoredMatch> = entries
            .iter()
            .map(|entry| ScoredMatch {
                item: Item {
                    id: entry.term.clone(),
                    context: MenuContext::WorkspaceTerms,
                    label: entry.term.clone(),
                    detail: None,
                    replacement: entry.term.clone(),
                },
                score: entry.weight,
                indices: Vec::new(),
            })
            .collect();

        results.sort_by(|a, b| {
            let len_cmp = a.item.replacement.len().cmp(&b.item.replacement.len());
            if len_cmp == Ordering::Equal {
                a.item.replacement.cmp(&b.item.replacement)
            } else {
                len_cmp
            }
        });

        if results.is_empty() {
            self.clear_terms_menu();
            return false;
        }

        self.vm.context = Some(MenuContext::WorkspaceTerms);
        self.vm.token = Some(token);
        self.vm.query = prefix;
        self.vm.results = results;
        self.vm.last_applied_id += 1;

        if let Some(prev_id) = previous_selected_id {
            if let Some(pos) = self.vm.results.iter().position(|res| res.item.id == prev_id) {
                self.vm.selected = pos;
            } else if self.vm.selected >= self.vm.results.len() {
                self.vm.selected = self.vm.results.len().saturating_sub(1);
            }
        } else if self.vm.selected >= self.vm.results.len() {
            self.vm.selected = self.vm.results.len().saturating_sub(1);
        }

        if self.vm.results.is_empty() {
            self.clear_terms_menu();
            return false;
        }

        self.vm.open = true;
        self.update_menu_ghost()
    }

    fn update_plaintext_ghost(&mut self, textarea: &TextArea<'_>) -> bool {
        if self.vm.open && !matches!(self.vm.context, Some(MenuContext::WorkspaceTerms)) {
            return self.set_ghost_state(None);
        }

        let Some((token, prefix)) = extract_plain_prefix(textarea) else {
            self.clear_terms_menu();
            return self.set_ghost_state(None);
        };

        if prefix.chars().count() < 2 {
            self.clear_terms_menu();
            return self.set_ghost_state(None);
        }

        let outcome = self.dependencies.workspace_terms.lookup(&prefix, MAX_RESULTS.min(64));

        let LookupOutcome {
            shared_extension,
            shortest_completion,
            entries,
        } = outcome;

        if self.terms_menu_enabled && !entries.is_empty() {
            if self.open_terms_menu(token.clone(), prefix.clone(), &entries) {
                return true;
            }
        } else {
            self.clear_terms_menu();
        }

        if shared_extension.is_empty() && shortest_completion.is_empty() {
            return self.set_ghost_state(None);
        }

        let completion_extension = if shortest_completion.is_empty() {
            shared_extension.clone()
        } else {
            shortest_completion
        };

        let ghost = GhostState {
            token: token.clone(),
            typed_len: prefix.chars().count(),
            shared_extension,
            completion_extension,
            source: GhostSource::Terms,
        };
        self.set_ghost_state(Some(ghost))
    }

    pub fn after_textarea_change(&mut self, textarea: &TextArea<'_>, needs_redraw: &mut bool) {
        if self.suspended {
            return;
        }
        if let Some((trigger, token, query)) = extract_token(textarea) {
            let current_context = self.vm.context;
            let same_trigger = current_context == Some(MenuContext::Trigger(trigger));

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

                let ghost_changed = self.update_menu_ghost();
                if was_open != self.vm.open || query_changed || ghost_changed {
                    *needs_redraw = true;
                }
            } else {
                // New trigger context - reset selection and perform search
                self.vm.selected = 0;
                self.vm.context = Some(MenuContext::Trigger(trigger));
                self.vm.token = Some(token);
                self.vm.query = query.clone();
                self.vm.open = false;

                // Perform search directly using enumerators
                self.perform_search(trigger, query);
                let ghost_changed = self.update_menu_ghost();
                if ghost_changed {
                    *needs_redraw = true;
                }
                *needs_redraw = true;
            }
        } else {
            let was_open = self.vm.open;
            let prev_context = self.vm.context;
            if matches!(self.vm.context, Some(MenuContext::Trigger(_))) {
                self.vm.open = false;
                self.vm.context = None;
                self.vm.results.clear();
                self.vm.selected = 0;
                self.vm.token = None;
                self.vm.query.clear();
            }
            if self.update_plaintext_ghost(textarea) {
                *needs_redraw = true;
            } else if was_open {
                *needs_redraw = true;
            }
            if was_open != self.vm.open || prev_context != self.vm.context {
                *needs_redraw = true;
            }
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
                            context: MenuContext::Trigger(trigger),
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

        let previous_selected_id =
            self.vm.results.get(self.vm.selected).map(|item| item.item.id.clone());

        // Update results
        self.vm.results = results;
        self.vm.last_applied_id += 1;

        // Update selection bounds
        if let Some(prev_id) = previous_selected_id {
            if let Some(pos) = self.vm.results.iter().position(|res| res.item.id == prev_id) {
                self.vm.selected = pos;
            } else if self.vm.selected >= self.vm.results.len() {
                self.vm.selected = self.vm.results.len().saturating_sub(1);
            }
        } else if self.vm.selected >= self.vm.results.len() {
            self.vm.selected = self.vm.results.len().saturating_sub(1);
        }
        if self.vm.results.is_empty() {
            self.vm.selected = 0;
        }

        self.update_menu_ghost();

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

        let context = self.vm.context?;
        if self.vm.results.is_empty() {
            return None;
        }

        let len = self.vm.results.len();
        let selected = self.vm.selected.min(len.saturating_sub(1));

        Some(AutocompleteMenuState {
            context,
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
        self.vm.context = None;
        self.vm.results.clear();
        self.vm.selected = 0;
        self.vm.token = None;
        self.vm.query.clear();
        let ghost_changed = self.set_ghost_state(None);
        if was_open || ghost_changed {
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

fn extract_plain_prefix(textarea: &TextArea<'_>) -> Option<(TokenPosition, String)> {
    let (row, col) = textarea.cursor();
    let line = textarea.lines().get(row)?;

    if line.is_empty() || col == 0 {
        return None;
    }

    let positions: Vec<(usize, usize, char)> = line
        .char_indices()
        .enumerate()
        .map(|(char_idx, (byte_idx, ch))| (char_idx, byte_idx, ch))
        .collect();

    let char_count = positions.len();
    let mut cursor_byte = line.len();
    if col < char_count {
        cursor_byte = positions[col].1;
    }

    let mut start_char = col.min(char_count);
    let mut start_byte = cursor_byte;

    while start_char > 0 {
        let (_, byte_idx, ch) = positions[start_char - 1];
        if is_token_boundary(ch) {
            break;
        }
        start_char -= 1;
        start_byte = byte_idx;
    }

    if start_char == col {
        return None;
    }

    let prefix = line[start_byte..cursor_byte].to_string();
    if prefix.is_empty() {
        return None;
    }

    Some((
        TokenPosition {
            row,
            start_col: start_char,
        },
        prefix,
    ))
}

fn compute_remainder(candidate: &str, typed_prefix: &str) -> Option<String> {
    if candidate.starts_with(typed_prefix) {
        let skip = typed_prefix.chars().count();
        Some(candidate.chars().skip(skip).collect())
    } else {
        let candidate_lower = candidate.to_lowercase();
        let typed_lower = typed_prefix.to_lowercase();
        if candidate_lower.starts_with(&typed_lower) {
            let skip = typed_prefix.chars().count();
            Some(candidate.chars().skip(skip).collect())
        } else {
            None
        }
    }
}
