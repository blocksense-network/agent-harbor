use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use once_cell::sync::OnceCell;
use tui_textarea::TextArea;
use unicode_segmentation::UnicodeSegmentation as _;

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

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub trigger: Trigger,
    pub label: String,
    pub detail: Option<String>,
    pub replacement: String,
}

pub trait Provider: Send + Sync {
    fn trigger(&self) -> Trigger;
    fn items(&self) -> Arc<Vec<Item>>;
}

struct StaticProvider {
    trigger: Trigger,
    items: Arc<Vec<Item>>,
}

impl StaticProvider {
    fn new(trigger: Trigger, items: Vec<Item>) -> Self {
        Self {
            trigger,
            items: Arc::new(items),
        }
    }
}

impl Provider for StaticProvider {
    fn trigger(&self) -> Trigger {
        self.trigger
    }

    fn items(&self) -> Arc<Vec<Item>> {
        Arc::clone(&self.items)
    }
}

struct GitFileProvider {
    cached: OnceCell<Arc<Vec<Item>>>,
}

impl GitFileProvider {
    fn new() -> Self {
        Self {
            cached: OnceCell::new(),
        }
    }

    fn load_items() -> Vec<Item> {
        let output = Command::new("git").args(["ls-files"]).output().ok();

        let stdout = match output {
            Some(out) if out.status.success() => out.stdout,
            _ => return Vec::new(),
        };

        let mut items = Vec::new();
        for line in String::from_utf8_lossy(&stdout).lines() {
            if line.is_empty() {
                continue;
            }
            items.push(Item {
                id: line.to_string(),
                trigger: Trigger::At,
                label: line.to_string(),
                detail: Some("Tracked file".to_string()),
                replacement: format!("@{}", line),
            });
        }
        items
    }
}

impl Provider for GitFileProvider {
    fn trigger(&self) -> Trigger {
        Trigger::At
    }

    fn items(&self) -> Arc<Vec<Item>> {
        Arc::clone(self.cached.get_or_init(|| Arc::new(Self::load_items())))
    }
}

struct WorkflowProvider {
    cached: OnceCell<Arc<Vec<Item>>>,
}

impl WorkflowProvider {
    fn new() -> Self {
        Self {
            cached: OnceCell::new(),
        }
    }

    fn load_items() -> Vec<Item> {
        let mut seen = HashSet::new();
        let mut items = Vec::new();

        // Add some basic workflow commands for the PoC
        // TODO: Replace with real WorkspaceWorkflowsEnumerator integration
        let poc_commands = vec![
            ("front-end-task", "Frontend development tasks"),
            ("back-end-task", "Backend development tasks"),
            ("test-setup", "Testing environment setup"),
        ];

        for (name, desc) in poc_commands {
            if seen.insert(name.to_string()) {
                items.push(Item {
                    id: name.to_string(),
                    trigger: Trigger::Slash,
                    label: name.to_string(),
                    detail: Some(desc.to_string()),
                    replacement: format!("/{}", name),
                });
            }
        }

        // Also include PATH executables
        let path_var = match env::var_os("PATH") {
            Some(p) => p,
            None => return items,
        };

        for dir in env::split_paths(&path_var) {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !is_executable(&path) {
                        continue;
                    }
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if seen.insert(name.to_string()) {
                            items.push(Item {
                                id: name.to_string(),
                                trigger: Trigger::Slash,
                                label: name.to_string(),
                                detail: Some(dir.display().to_string()),
                                replacement: format!("/{}", name),
                            });
                        }
                    }
                }
            }
        }

        items
    }
}

impl Provider for WorkflowProvider {
    fn trigger(&self) -> Trigger {
        Trigger::Slash
    }

    fn items(&self) -> Arc<Vec<Item>> {
        Arc::clone(self.cached.get_or_init(|| Arc::new(Self::load_items())))
    }
}

fn is_executable(path: &PathBuf) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        if let Ok(metadata) = fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        false
    }

    #[cfg(not(unix))]
    {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "exe" | "bat" | "cmd" | "com"
                )
            })
            .unwrap_or(false)
    }
}

#[derive(Debug, Default)]
struct InlineMenuViewModel {
    open: bool,
    trigger: Option<Trigger>,
    query: String,
    selected: usize,
    results: Vec<ScoredMatch>,
    token: Option<TokenPosition>,
    last_applied_id: u64,
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

#[derive(Debug)]
pub struct InlineAutocomplete {
    vm: InlineMenuViewModel,
    tx_query: Sender<QueryMessage>,
    rx_result: Receiver<ResultMessage>,
    pending: Option<PendingQuery>,
    next_request_id: u64,
    show_border: bool,
    suspended: bool,
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

#[derive(Debug)]
struct PendingQuery {
    id: u64,
    trigger: Trigger,
    needle: String,
    deadline: Instant,
}

enum QueryMessage {
    Search {
        id: u64,
        trigger: Trigger,
        needle: String,
    },
}

struct ResultMessage {
    id: u64,
    matches: Vec<ScoredMatch>,
}

impl InlineAutocomplete {
    pub fn new() -> Self {
        Self::with_providers(vec![
            Arc::new(WorkflowProvider::new()) as Arc<dyn Provider>,
            Arc::new(GitFileProvider::new()) as Arc<dyn Provider>,
            Arc::new(StaticProvider::new(
                Trigger::At,
                vec![
                    Item {
                        id: "specs/Public/WebUI-PRD.md".to_string(),
                        trigger: Trigger::At,
                        label: "specs/Public/WebUI-PRD.md".to_string(),
                        detail: Some("Product requirements".to_string()),
                        replacement: "@specs/Public/WebUI-PRD.md".to_string(),
                    },
                    Item {
                        id: "PoC/tui-exploration/src/main.rs".to_string(),
                        trigger: Trigger::At,
                        label: "PoC/tui-exploration/src/main.rs".to_string(),
                        detail: Some("TUI playground".to_string()),
                        replacement: "@PoC/tui-exploration/src/main.rs".to_string(),
                    },
                ],
            )),
            Arc::new(StaticProvider::new(
                Trigger::Slash,
                vec![Item {
                    id: "ah".to_string(),
                    trigger: Trigger::Slash,
                    label: "ah".to_string(),
                    detail: Some("Agent Harbor CLI".to_string()),
                    replacement: "/ah".to_string(),
                }],
            )),
        ])
    }

    pub fn with_providers(providers: Vec<Arc<dyn Provider>>) -> Self {
        let (tx_query, rx_query) = crossbeam_channel::unbounded();
        let (tx_result, rx_result) = crossbeam_channel::unbounded();
        spawn_worker(providers, rx_query, tx_result);

        Self {
            vm: InlineMenuViewModel::default(),
            tx_query,
            rx_result,
            pending: None,
            next_request_id: 1,
            show_border: true,
            suspended: false,
        }
    }

    pub fn set_show_border(&mut self, value: bool) {
        self.show_border = value;
    }

    pub fn notify_text_input(&mut self) {
        self.suspended = false;
    }

    pub fn handle_key_event(
        &mut self,
        key: &KeyEvent,
        textarea: &mut TextArea<'_>,
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
                let changed = self.commit_selection(textarea);
                AutocompleteKeyResult::Consumed {
                    text_changed: changed,
                }
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                let changed = self.commit_selection(textarea);
                AutocompleteKeyResult::Consumed {
                    text_changed: changed,
                }
            }
            KeyCode::Esc => {
                self.close();
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
        self.vm.open && !self.vm.results.is_empty()
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

    pub fn after_textarea_change(&mut self, textarea: &TextArea<'_>) {
        if self.suspended {
            return;
        }
        if let Some((trigger, token, query)) = extract_token(textarea) {
            let same_trigger = self.vm.trigger == Some(trigger);
            let same_token = self.vm.token.as_ref() == Some(&token);
            let same_query = self.vm.query == query;

            if same_trigger && same_token && same_query {
                // No textual change – keep the menu visible without rescheduling work
                self.vm.open = !self.vm.results.is_empty();
                return;
            }

            if !same_trigger || !same_token || !same_query {
                self.vm.selected = 0;
            }
            self.vm.trigger = Some(trigger);
            self.vm.token = Some(token);
            self.vm.query = query.clone();
            self.vm.open = false;
            self.schedule_query(trigger, query);
        } else {
            self.close();
        }
    }

    pub fn poll_results(&mut self) {
        while let Ok(msg) = self.rx_result.try_recv() {
            if self.suspended {
                continue;
            }
            if msg.id < self.vm.last_applied_id {
                continue;
            }
            self.vm.results = msg.matches;
            self.vm.last_applied_id = msg.id;
            if self.vm.selected >= self.vm.results.len() {
                self.vm.selected = self.vm.results.len().saturating_sub(1);
            }
            self.constrain_selected();
            let query = self.vm.query.trim();
            let only_exact = self.vm.results.len() == 1
                && !query.is_empty()
                && self.vm.results[0].item.label.eq_ignore_ascii_case(query);

            if self.vm.results.is_empty() || only_exact {
                self.vm.results.clear();
                self.vm.open = false;
            } else {
                self.vm.open = true;
            }
        }
    }

    pub fn on_tick(&mut self) {
        if let Some(pending) = self.pending.take() {
            if Instant::now() >= pending.deadline {
                let _ = self.tx_query.send(QueryMessage::Search {
                    id: pending.id,
                    trigger: pending.trigger,
                    needle: pending.needle,
                });
            } else {
                self.pending = Some(pending);
            }
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

    fn schedule_query(&mut self, trigger: Trigger, needle: String) {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.pending = Some(PendingQuery {
            id,
            trigger,
            needle,
            deadline: Instant::now() + QUERY_DEBOUNCE,
        });
    }

    fn commit_selection(&mut self, textarea: &mut TextArea<'_>) -> bool {
        if self.vm.results.is_empty() {
            self.close();
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

        self.close();
        changed
    }

    /// Close the autocomplete menu and clear any pending state.
    pub fn close(&mut self) {
        self.vm.open = false;
        self.vm.trigger = None;
        self.vm.results.clear();
        self.vm.selected = 0;
        self.vm.token = None;
        self.pending = None;
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

fn spawn_worker(
    providers: Vec<Arc<dyn Provider>>,
    rx_query: Receiver<QueryMessage>,
    tx_result: Sender<ResultMessage>,
) {
    let mut provider_lookup: HashMap<Trigger, Vec<Arc<dyn Provider>>> = HashMap::new();
    for provider in providers {
        provider_lookup.entry(provider.trigger()).or_default().push(provider);
    }

    thread::Builder::new()
        .name("autocomplete-matcher".into())
        .spawn(move || {
            let mut config = Config::DEFAULT;
            config.normalize = true;
            config.ignore_case = true;
            config.prefer_prefix = false;
            let mut matcher = Matcher::new(config);
            let mut scratch = Vec::new();
            while let Ok(message) = rx_query.recv() {
                match message {
                    QueryMessage::Search {
                        id,
                        trigger,
                        needle,
                    } => {
                        let mut aggregated = Vec::new();
                        let mut seen = HashSet::new();

                        if let Some(list) = provider_lookup.get(&trigger) {
                            for provider in list {
                                let matches =
                                    compute_matches(provider, &needle, &mut matcher, &mut scratch);
                                for m in matches {
                                    if seen.insert(m.item.id.clone()) {
                                        aggregated.push(m);
                                    }
                                }
                            }
                        }

                        aggregated.sort_by(|a, b| b.score.cmp(&a.score));
                        aggregated.truncate(MAX_RESULTS);

                        let _ = tx_result.send(ResultMessage {
                            id,
                            matches: aggregated,
                        });
                    }
                }
            }
        })
        .expect("failed to start autocomplete worker");
}

fn compute_matches(
    provider: &Arc<dyn Provider>,
    needle: &str,
    matcher: &mut Matcher,
    scratch: &mut Vec<u32>,
) -> Vec<ScoredMatch> {
    let items = provider.items();
    let mut seen_ids: HashSet<String> = HashSet::new();
    if needle.is_empty() {
        return items
            .iter()
            .filter(|item| seen_ids.insert(item.id.clone()))
            .take(MAX_RESULTS)
            .cloned()
            .map(|item| ScoredMatch {
                item,
                score: 0,
                indices: Vec::new(),
            })
            .collect();
    }

    let pattern = Pattern::parse(needle, CaseMatching::Ignore, Normalization::Smart);
    let mut utf32 = Vec::new();
    let mut matches = Vec::new();

    for item in items.iter() {
        if !seen_ids.insert(item.id.clone()) {
            continue;
        }
        scratch.clear();
        utf32.clear();
        if let Some(score) = pattern.indices(
            Utf32Str::new(item.label.as_str(), &mut utf32),
            matcher,
            scratch,
        ) {
            let mut indices: Vec<usize> = scratch.iter().map(|idx| *idx as usize).collect();
            indices.sort_unstable();
            indices.dedup();
            matches.push(ScoredMatch {
                item: item.clone(),
                score,
                indices,
            });
        }
    }

    matches.sort_by(|a, b| b.score.cmp(&a.score));
    matches.truncate(MAX_RESULTS);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tui_textarea::{CursorMove, TextArea};

    struct TestProvider {
        trigger: Trigger,
        items: Arc<Vec<Item>>,
    }

    impl TestProvider {
        fn new(trigger: Trigger, items: Vec<Item>) -> Self {
            Self {
                trigger,
                items: Arc::new(items),
            }
        }
    }

    impl Provider for TestProvider {
        fn trigger(&self) -> Trigger {
            self.trigger
        }

        fn items(&self) -> Arc<Vec<Item>> {
            Arc::clone(&self.items)
        }
    }

    #[test]
    fn detects_at_trigger_token() {
        let mut textarea = TextArea::from(["@main".to_string()]);
        textarea.move_cursor(CursorMove::End);

        let (trigger, token, query) = extract_token(&textarea).expect("token should be detected");
        assert_eq!(trigger, Trigger::At);
        assert_eq!(token.start_col, 0);
        assert_eq!(query, "main");
    }

    #[test]
    fn matcher_returns_ordered_results() {
        let provider = Arc::new(TestProvider::new(
            Trigger::At,
            vec![
                Item {
                    id: "1".to_string(),
                    trigger: Trigger::At,
                    label: "main.rs".to_string(),
                    detail: None,
                    replacement: "@main.rs".to_string(),
                },
                Item {
                    id: "2".to_string(),
                    trigger: Trigger::At,
                    label: "mod.rs".to_string(),
                    detail: None,
                    replacement: "@mod.rs".to_string(),
                },
            ],
        )) as Arc<dyn Provider>;

        let mut autocomplete = InlineAutocomplete::with_providers(vec![provider]);
        let mut textarea = TextArea::from(["@ma".to_string()]);
        textarea.move_cursor(CursorMove::End);
        autocomplete.after_textarea_change(&textarea);

        thread::sleep(QUERY_DEBOUNCE + Duration::from_millis(20));
        autocomplete.on_tick();
        thread::sleep(Duration::from_millis(20));
        autocomplete.poll_results();

        assert!(autocomplete.vm.open);
        assert!(!autocomplete.vm.results.is_empty());
        assert_eq!(autocomplete.vm.results[0].item.label, "main.rs");
    }

    #[test]
    fn commit_inserts_replacement() {
        let provider = Arc::new(TestProvider::new(Trigger::At, Vec::new())) as Arc<dyn Provider>;
        let mut autocomplete = InlineAutocomplete::with_providers(vec![provider]);

        autocomplete.vm.open = true;
        autocomplete.vm.trigger = Some(Trigger::At);
        autocomplete.vm.selected = 0;
        autocomplete.vm.token = Some(TokenPosition {
            row: 0,
            start_col: 0,
        });
        autocomplete.vm.results = vec![ScoredMatch {
            item: Item {
                id: "path".to_string(),
                trigger: Trigger::At,
                label: "gamma.rs".to_string(),
                detail: None,
                replacement: "@gamma.rs".to_string(),
            },
            score: 100,
            indices: vec![0, 1],
        }];

        let mut textarea = TextArea::from(["@ga".to_string()]);
        textarea.move_cursor(CursorMove::End);

        autocomplete.commit_selection(&mut textarea);

        assert_eq!(textarea.lines()[0], "@gamma.rs");
        assert!(!autocomplete.vm.open);
    }
}
