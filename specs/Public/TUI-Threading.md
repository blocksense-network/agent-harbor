# TUI Threading Model

## Overview

The TUI follows a strict threading model where exactly one OS thread owns all UI state and rendering. This thread runs a current-thread Tokio runtime with LocalSet to support !Send futures while maintaining thread safety. All other components communicate with the UI thread exclusively through channels.

## Core Principles

### Single UI Thread Ownership

**Exactly one OS thread "owns" the UI**: The terminal, Ratatui Frame, and all UI state (`Rc<RefCell<…>>`) live on this dedicated thread.

**Message passing is preferred**: All UI mutations occur on the owning thread through message passing via channels.

**Weak references for LocalSet tasks**: Async tasks running in the UI thread's LocalSet may hold weak references to UI view models for natural continuous updates, with automatic self-termination when UI elements are dropped.

## Thread & Runtime Structure

### Production Configuration

- **Dedicated OS Thread**: Spawn a dedicated OS thread for UI operations
- **Current-Thread Tokio Runtime**: Run a `tokio::runtime::Runtime` with `Runtime::new_current_thread()` on the UI thread
- **LocalSet Usage**: Use `tokio::task::LocalSet` to keep !Send futures local to the UI thread
- **Thread Naming**: Name the UI thread "tui-main" for debugging purposes

### Runtime Setup Example

```rust
// Spawn dedicated UI thread
let ui_thread = thread::spawn(|| {
    // Create current-thread runtime
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    // Run with LocalSet for !Send futures
    let local = tokio::task::LocalSet::new();

    local.block_on(&rt, async {
        run_ui_loop().await
    })
});
```

## Channel Architecture

### Message Types

#### UiMsg - General Mailbox

- **Channel Type**: `mpsc::UnboundedSender<UiMsg>`
- **Use Case**: General-purpose UI messages (task updates, user actions, state changes)
- **Why Unbounded**: Low ceremony, prevents backpressure from blocking background tasks
- **Example Messages**:
  - `TaskCreated(task_id, task_data)`
  - `TaskStatusChanged(task_id, new_status)`
  - `UserActionCompleted(action_id, result)`

#### Watch Channels - Last-Value Signals

- **Channel Type**: `tokio::sync::watch::Sender<T>`
- **Use Case**: High-frequency signals where only the latest value matters
- **Examples**:
  - Network connectivity status
  - Progress indicators
  - Real-time activity streams

#### Broadcast Channels - Multi-Subscriber Updates

- **Channel Type**: `tokio::sync::broadcast::Sender<T>`
- **Use Case**: Updates that may have multiple subscribers
- **Examples**:
  - Global configuration changes
  - Theme updates
  - Workspace file system changes

#### Oneshot Channels - Request/Response

- **Channel Type**: `tokio::sync::oneshot::Sender<T>`
- **Use Case**: Request/response patterns, especially for consuming non-async APIs
- **Examples**:
  - Configuration lookups
  - File system operations
  - External service calls

### Channel Ownership

- **UI Thread**: Owns all channel receivers
- **Background Tasks**: Own channel senders, never hold references to UI state
- **Message Direction**: Always background → UI thread, never bidirectional state sharing

## Event Handling

### User Input Thread

- **Dedicated Reader Thread**: Spawn a separate OS thread for reading terminal events
- **Message Passing**: Send all `crossterm::event::Event` instances to UI thread via channel
- **Thread Safety**: No shared state, only message passing

```rust
// Event reader thread (from dashboard_loop.rs pattern)
thread::spawn(move || {
    while let Ok(event) = crossterm::event::read() {
        let _ = event_sender.send(UiMsg::UserInput(event));
    }
});
```

### Event Processing

- **UI Thread Processing**: All event interpretation and state updates happen on UI thread
- **Async Integration**: Events processed within the LocalSet context
- **State Consistency**: Single-threaded access prevents race conditions

## Animation & Timing

### 60 FPS Ticker

- **Location**: Runs on the main UI thread within the LocalSet
- **Purpose**: Drives smooth animations and UI updates
- **Implementation**: Use `tokio::time::interval` with 16.67ms intervals
- **Integration**: Ticker events processed alongside user input events

```rust
let mut ticker = tokio::time::interval(Duration::from_millis(16)); // ~60 FPS
loop {
    tokio::select! {
        event = event_receiver.recv() => { /* handle events */ }
        _ = ticker.tick() => { /* handle animation frame */ }
    }
}
```

## Background Task Architecture

### Task Isolation

- **Channel Communication**: All communication through async channels is preferred
- **LocalSet Tasks**: Async tasks running in the UI thread's LocalSet may use weak references for continuous updates (i.e. `Weak<RefCell<…>>`)
- **Self-Termination**: Tasks with weak references must halt when references cannot be upgraded (UI element dropped)
- **Error Handling**: Background task errors sent as messages to UI thread
- **Cancellation**: Use `tokio::sync::CancellationToken` for clean shutdown

### Background Task Examples

1. **File System Monitoring**:
   - Runs on background thread
   - Sends `UiMsg::FileSystemChanged(changes)` messages
   - No direct UI mutation

2. **Task Execution Monitoring**:
   - Monitors external processes
   - Sends `UiMsg::TaskActivity(task_id, activity_data)` messages
   - Uses watch channels for progress updates

3. **Workspace Indexing**:
   - Builds file/directory caches
   - Sends `UiMsg::WorkspaceIndexed(index_data)` messages
   - Updates via broadcast for multiple UI components

4. **LocalSet Activity Streaming** (uses weak references):
   - Runs as async task in UI thread's LocalSet
   - Holds `Weak<RefCell<TaskViewModel>>` to target task
   - Continuously updates activity display via direct weak reference upgrades
   - Self-terminates when weak reference upgrade fails (task UI dropped)
   - Pattern: `if let Some(task_vm) = weak_task.upgrade() { task_vm.borrow_mut().update_activity(data); } else { break; }`

## Weak Reference Pattern

### When to Use Weak References

Weak references are appropriate for LocalSet async tasks that need to continuously update specific UI elements in a natural way. This pattern is commonly used for:

- Real-time activity streaming to task cards
- Progress indicators with frequent updates
- Live data feeds to specific UI components

### Weak Reference Rules

- **LocalSet Only**: Only tasks running in the UI thread's LocalSet may hold weak references
- **UI Thread Context**: Weak reference upgrades and mutations must occur on the UI thread
- **Self-Termination**: Tasks must check weak reference validity and terminate when upgrade fails
- **No Strong Cycles**: Never create strong reference cycles between UI elements and background tasks

### Example Pattern

```rust
async fn stream_activity_updates(
    weak_task_vm: Weak<RefCell<TaskViewModel>>,
    activity_stream: impl Stream<Item = ActivityEvent>,
) {
    pin_mut!(activity_stream);

    while let Some(event) = activity_stream.next().await {
        // Attempt to upgrade weak reference
        if let Some(task_vm) = weak_task_vm.upgrade() {
            // UI element still exists - update it
            task_vm.borrow_mut().push_activity(event);
        } else {
            // UI element has been dropped - terminate task
            break;
        }

        // Yield control to allow UI updates
        tokio::task::yield_now().await;
    }
}
```

## Error Handling & Recovery

### Thread Failure Isolation

- **Background Thread Failures**: Don't crash the UI thread
- **Error Messages**: Send error information via channels
- **Graceful Degradation**: UI continues operating even if background tasks fail
- **Logging**: All errors logged with appropriate context

### Cleanup & Shutdown

- **Cancellation Tokens**: Use `tokio::sync::CancellationToken` for coordinated shutdown
- **Channel Closure**: Properly close channels during shutdown
- **Terminal Cleanup**: Ensure terminal state restoration on exit
- **Thread Join**: Wait for background threads during shutdown

## Performance Considerations

### Channel Selection Guidelines

- **Unbounded Channels**: Use for low-frequency, high-importance messages
- **Bounded Channels**: Consider for high-frequency messages to prevent unbounded growth
- **Watch Channels**: Optimal for frequently-changing single values
- **Broadcast Channels**: Use sparingly, only when multiple subscribers are needed

### Memory Management

- **Message Size**: Keep messages small to reduce channel overhead
- **Backpressure**: Monitor channel sizes in development builds
- **Leak Prevention**: Ensure all channel receivers are properly consumed
