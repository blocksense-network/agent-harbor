Ok let me give you the content

You’re right—I initially focused on the “focus” variants’ UX in the TUI. For the split-layout part: selecting h/H (horizontal split) is being parsed and forwarded in the TUI, but nothing ever turns that into a LayoutConfig because the multiplexer layer isn’t invoked from the launch path. That’s why it doesn’t affect LayoutConfig.

What the code actually does

1) The TUI maps the action strings to SplitMode and focus and puts them in TaskLaunchParams:
```rust name=crates/ah-tui/src/view_model/agents_selector_model.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui/src/view_model/agents_selector_model.rs#L1642-L1660
// Parse the selected option to determine split mode and focus
let (split_mode, focus) = match selected_option.as_str() {
    "Launch in new tab (t)" => (ah_core::SplitMode::None, false),
    "Launch in split view (s)" => (ah_core::SplitMode::Auto, false),
    "Launch in horizontal split (h)" => (ah_core::SplitMode::Horizontal, false),
    "Launch in vertical split (v)" => (ah_core::SplitMode::Vertical, false),
    "Launch in new tab and focus (T)" => (ah_core::SplitMode::None, true),
    "Launch in split view and focus (S)" => (ah_core::SplitMode::Auto, true),
    "Launch in horizontal split and focus (H)" => (ah_core::SplitMode::Horizontal, true),
    "Launch in vertical split and focus (V)" => (ah_core::SplitMode::Vertical, true),
    _ => (ah_core::SplitMode::Auto, false),
};
```

And those make it into the launch params:
```rust name=crates/ah-tui/src/view_model/agents_selector_model.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui/src/view_model/agents_selector_model.rs#L1439-L1449
let mut builder = ah_core::task_manager::TaskLaunchParams::builder()
    .starting_point(starting_point)
    .working_copy_mode(working_copy_mode)
    .description(description.clone())
    .agents(card_agents.clone())
    .agent_type(agent_type)
    .split_mode(split_mode)
    .focus(focus)
    .record(true);
```

2) The multiplexer layer (where LayoutConfig lives) interprets SplitMode when it is actually called:
```rust name=crates/ah-tui-multiplexer/src/lib.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui-multiplexer/src/lib.rs#L26-L36
/// Standard AH layout configuration
#[derive(Debug, Clone)]
pub struct LayoutConfig<'a> {
    pub task_id: &'a str,
    pub working_dir: &'a std::path::Path,
    pub editor_cmd: Option<&'a str>,
    pub agent_cmd: &'a str,
    pub log_cmd: Option<&'a str>,
    pub split_mode: SplitMode,    // How to split the view
    pub focus: bool,              // Whether to switch focus to the new task window/pane
}
```

And uses it like this:
```rust name=crates/ah-tui-multiplexer/src/lib.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui-multiplexer/src/lib.rs#L69-L115
pub fn create_task_layout(&self, config: &LayoutConfig) -> Result<LayoutHandle, AwMuxError> {
    let title = format!("ah-task-{}", config.task_id);

    let window_id = match config.split_mode {
        SplitMode::None => { /* open new window */ }
        SplitMode::Auto | SplitMode::Horizontal | SplitMode::Vertical => {
            // Split the current window
            self.mux.current_window()?.ok_or_else(|| {
                AwMuxError::Layout(
                    "Not running in a multiplexer window, cannot create split view".to_string(),
                )
            })?
        }
    };

    // Split for agent pane
    let split_direction = match config.split_mode {
        SplitMode::Horizontal => SplitDirection::Horizontal,
        SplitMode::Vertical => SplitDirection::Vertical,
        SplitMode::Auto | SplitMode::None => SplitDirection::Horizontal, // default
    };
    let agent_pane = self.mux.split_pane(/* ... */, split_direction, /* ... */)?;
    /* ... */
}
```

3) But the actual app path you’re using never constructs a LayoutConfig or calls create_task_layout. The TUI just builds TaskLaunchParams and hands them to the task manager. Also, the multiplexer facade itself is currently not available by default:
```rust name=crates/ah-tui-multiplexer/src/lib.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui-multiplexer/src/lib.rs#L213-L219
pub fn default_multiplexer() -> Result<Box<dyn Multiplexer + Send + Sync>, AwMuxError> {
    // This will be implemented when ah-mux provides the multiplexer implementations
    Err(AwMuxError::Config(
        "No multiplexer implementations available yet".to_string(),
    ))
}
```

So, selecting h/H does change the SplitMode in the launch params, but nothing in the current launch flow converts that into a LayoutConfig or calls the multiplexer to actually split a window. As a result, your selection can’t “affect” LayoutConfig—it’s never constructed or used during your run.

Here’s what’s happening end to end, and why your selection isn’t visibly affecting anything.

1) The TUI correctly maps your choice to SplitMode and focus
- When you select t/s/h/v (or uppercase), the TUI parses the string and turns it into ah_core::SplitMode plus a focus flag.
```rust name=crates/ah-tui/src/view_model/agents_selector_model.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui/src/view_model/agents_selector_model.rs#L1642-L1660
// Parse the selected option to determine split mode and focus
let (split_mode, focus) = match selected_option.as_str() {
    "Launch in new tab (t)" => (ah_core::SplitMode::None, false),
    "Launch in split view (s)" => (ah_core::SplitMode::Auto, false),
    "Launch in horizontal split (h)" => (ah_core::SplitMode::Horizontal, false),
    "Launch in vertical split (v)" => (ah_core::SplitMode::Vertical, false),
    "Launch in new tab and focus (T)" => (ah_core::SplitMode::None, true),
    "Launch in split view and focus (S)" => (ah_core::SplitMode::Auto, true),
    "Launch in horizontal split and focus (H)" => (ah_core::SplitMode::Horizontal, true),
    "Launch in vertical split and focus (V)" => (ah_core::SplitMode::Vertical, true),
    _ => (ah_core::SplitMode::Auto, false),
};
```

- Those values are put into TaskLaunchParams that the TaskManager receives.
```rust name=crates/ah-tui/src/view_model/agents_selector_model.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui/src/view_model/agents_selector_model.rs#L1439-L1449
let mut builder = ah_core::task_manager::TaskLaunchParams::builder()
    .starting_point(starting_point)
    .working_copy_mode(working_copy_mode)
    .description(description.clone())
    .agents(card_agents.clone())
    .agent_type(agent_type)
    .split_mode(split_mode)
    .focus(focus)
    .record(true);
```

2) The local TaskManager passes them into LayoutConfig and calls the multiplexer
- In the local (generic) TaskManager implementation, your split_mode and focus are forwarded into a LayoutConfig and used to create the multiplexer layout:
```rust name=crates/ah-core/src/local_task_manager.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-core/src/local_task_manager.rs#L530-L547
let layout_config = LayoutConfig {
    task_id: &session_id,
    working_dir: &current_dir,
    editor_cmd: Some("lazygit"),
    agent_cmd: &agent_cmd_inner,
    log_cmd: None,
    split_mode: *params.split_mode(),
    focus: params.focus(),
};

match self.multiplexer.create_task_layout(&layout_config) {
    Ok(_layout_handle) => { /* success */ }
    Err(e) => { /* error handling */ }
}
```

3) The multiplexer honors SplitMode and focus, but only inside a multiplexer session
- The LayoutConfig is consumed here. Notice how SplitMode affects which operation is attempted:
```rust name=crates/ah-tui-multiplexer/src/lib.rs url=https://github.com/blocksense-network/agent-harbor/blob/1c31228a0c67ed829510aa9cfefd3b6910294c32/crates/ah-tui-multiplexer/src/lib.rs#L69-L115
pub fn create_task_layout(&self, config: &LayoutConfig) -> Result<LayoutHandle, AwMuxError> {
    let title = format!("ah-task-{}", config.task_id);

    let window_id = match config.split_mode {
        SplitMode::None => {
            // Create new window
            let window_opts = WindowOptions { /* ... */ focus: true, };
            self.mux.open_window(&window_opts)?
        }
        SplitMode::Auto | SplitMode::Horizontal | SplitMode::Vertical => {
            // Split the current window (must already be in a multiplexer window)
            self.mux.current_window()?.ok_or_else(|| {
                AwMuxError::Layout("Not running in a multiplexer window, cannot create split view".to_string())
            })?
        }
    };

    // Split for agent pane
    let split_direction = match config.split_mode {
        SplitMode::Horizontal => SplitDirection::Horizontal,
        SplitMode::Vertical => SplitDirection::Vertical,
        SplitMode::Auto | SplitMode::None => SplitDirection::Horizontal, // default
    };

    let agent_pane = self.mux.split_pane(/* split_direction, ... */)?;
    /* focus handling uses config.focus */
}
```

Why you don’t see any effect

- For “focus” in the TUI: the ViewModel sets focus on the draft card after launching, so in-TUI focus doesn’t change even if the multiplexer focuses the new pane/window.

- For split orientation (e.g., picking h/H): the multiplexer only applies the split if the app is already running inside a multiplexer window and a concrete multiplexer backend is present. If there’s no active multiplexer session (or no backend wired in), create_task_layout returns an error like “Not running in a multiplexer window, cannot create split view,” and your selection won’t have visible effect in the terminal. Additionally, the default multiplexer factory currently yields “No multiplexer implementations available yet,” so unless a concrete multiplexer is provided, LayoutConfig can’t be acted on.

In short:
- Your selection is correctly parsed and forwarded all the way to LayoutConfig.
- The actual split/focus effect happens only via the multiplexer; if the app isn’t running under a multiplexer (or no backend is wired), nothing changes on-screen.
- Independently, the TUI’s own focus is reset to the draft card after launch, so “Launch in new tab and focus” won’t change the TUI selection even though the multiplexer might focus the new pane/window.

