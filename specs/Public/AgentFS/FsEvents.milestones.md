Here’s a concrete, test-driven development plan to implement the full “shim ↔ daemon ↔ FsCore” event pipeline (kqueue/FSEvents interception, daemon-driven EVFILT_USER doorbells, kernel-like EVFILT_VNODE synthesis, and FsCore event triggers). Each milestone ends with **fully automated** verification steps you can wire into `cargo test` on macOS runners.

# Milestone 0 — Groundwork: shared types, feature flags, crates COMPLETED

**Goal:** Create a clean skeleton so later milestones snap in without churn.

- **Crates/Modules**
  - `agentfs-core`: already defines event model; expose `EventKind`, `EventSink`, and a subscription API behind a feature `events` (enabled by default).
  - `agentfs-interpose-shim`: DYLD interposer that already handshakes with the daemon over a UNIX socket; we'll add kqueue interception and event injection here.
  - `agentfs-daemon`: control plane + FsCore host; add "watch service" (registry + fanout + doorbells).
  - `agentfs-proto`: SSZ/JSON message types covering:
    - `WatchRegisterKqueue` (per kqueue fd + per-fd filter registrations)
    - `WatchRegisterFSEvents` (stream params)
    - `WatchUnregister`
    - `WatchDoorbell` (identify doorbell for a kqueue, see M3)
    - `FsEventBroadcast` (FsCore → daemon → shim)

- **Acceptance (automated)**
  - Build succeeds with the feature `events`.
  - Shim loads & handshakes (already covered in existing tests).

- **Implementation Details:**
  - `events` feature flag defined and enabled by default in `agentfs-core/Cargo.toml`
  - All SSZ message types implemented in `agentfs-proto/src/messages.rs` with proper serialization
  - Watch service skeleton implemented in `agentfs-daemon/src/watch_service.rs`
  - DYLD interposer handshake functionality verified through existing tests

- **Verification Results:**
  - [x] Build succeeds with the feature `events`
  - [x] Shim loads & handshakes work correctly (verified through existing tests)
  - [x] All SSZ message types defined and compilable
  - [x] Watch service structure in place

# Milestone 1 — FsCore Event Bus (publish/subscribe)

**Goal:** Deterministic in-memory FsCore event triggers that match our overlay semantics.

- **API (Rust)**
  - `FsCore::subscribe_events(&self, sink: Arc<dyn EventSink>) -> SubscriptionId`
  - `FsCore::unsubscribe_events(SubscriptionId)`
  - Emit `EventKind::{Created, Removed, Modified, Renamed}` at the points where creates, unlink, writes/truncates/metadata, and rename occur. (Events for `BranchCreated`, `SnapshotCreated` already modeled; keep them but they won’t map to EVFILT_VNODE.)

- **Mapping spec (FsCore → generic)**
  - Created → {path}
  - Removed → {path}
  - Modified → {path}
  - Renamed → {from,to}

- **Acceptance (unit tests)**
  - In-memory backstore (`BackstoreMode::InMemory`) scenario:
    - create/write/rename/unlink yield the expected `EventKind` sequence with **no I/O to disk**.

  - Existing overlay tests remain green (no regressions).

**Milestone 1 — FsCore Event Bus** COMPLETED

- **Deliverables:**
  - Exposed `EventKind`, `EventSink`, and subscription API in `agentfs-core` behind `events` feature flag (enabled by default)
  - Implemented `FsCore::subscribe_events(&self, sink: Arc<dyn EventSink>) -> SubscriptionId`
  - Implemented `FsCore::unsubscribe_events(SubscriptionId)`
  - Added event emission for `EventKind::{Created, Removed, Modified, Renamed}` at filesystem operation points
  - Events only emitted when `track_events` is enabled in `FsConfig`
  - Created comprehensive unit tests verifying event emission for all relevant operations
  - Added milestone-specific test for create/write/rename/unlink sequence in in-memory backstore scenario

- **Implementation Details:**
  - **Event Infrastructure**: Added `EventKind::{Created, Removed, Modified, Renamed}` to `crates/agentfs-core/src/types.rs`
  - **Event Sink Trait**: Implemented `EventSink` trait with `on_event(&self, &EventKind)` method
  - **Subscription API**: Extended `FsCore` struct with `event_subscriptions: Mutex<HashMap<SubscriptionId, Arc<dyn EventSink>>>` and `next_subscription_id: Mutex<u64>`
  - **Event Emission**: Integrated event emission into all state-changing filesystem operations (create, write, rename, unlink, set_mode, set_owner, set_times, xattr_set, xattr_remove, ftruncate)
  - **Path Resolution**: Modified `Handle` struct to store resolved paths (`pub path: PathBuf`) for efficient event emission
  - **Thread Safety**: Used `Mutex` for concurrent access to event subscriptions
  - **Event Filtering**: Events only emitted when `FsConfig.track_events` is true

- **Key Source Files:**
  - `crates/agentfs-core/src/types.rs`: Event types and EventSink trait definitions
  - `crates/agentfs-core/src/vfs.rs`: FsCore event subscription API and event emission logic
  - `crates/agentfs-core/src/lib.rs`: Comprehensive unit tests for event emission

- **Verification Results:**
  - [x] In-memory backstore scenario: create/write/rename/unlink yield expected `EventKind` sequence with no I/O to disk
  - [x] Existing overlay tests remain green (no regressions)
  - [x] Event subscription/unsubscribe works correctly
  - [x] Events only emitted when `track_events` is enabled
  - [x] All filesystem operations emit appropriate event types
  - [x] Thread-safe event handling with proper mutex usage

# Milestone 2 — Daemon "watch service" + event fanout COMPLETED

**Goal:** Central registry in FsCore that knows which target-process watches (kqueue/FSEvents) are interested in which paths and fans out FsCore events.

- **Core registry responsibilities**
  - Maintain per-process **watch table**:
    - **Kqueue watches:** `<proc_pid, kq_id, fd, path, vnode_flags>` (store path with each watch registration).
    - **FSEvents watches:** `<proc_pid, stream_id, path_prefixes, flags>` (high-level).

  - Subscribe to FsCore (M1) and translate `EventKind` → abstract watch hits (path prefix & equality checks).

- **Interpose shim responsibilities**
  - Intercept FS monitoring API calls and forward them with suitable IPC operations to the daemon.

- **Daemon responsibilities**
  - Accepts IPC requests and delegates the implementation to the FsCore registry.

- **Acceptance (integration tests; no shim yet)**
  - Spawn daemon with **fake watches** and feed it synthetic `EventKind`; it resolves recipients correctly:
    - Created/Removed/Modified hits matching kqueue fd path.
    - Prefix matching for FSEvents streams (later used in M6).

  - Verify no hits for unregistered or whiteout-hidden targets (uses existing overlay/whiteout semantics in core).

**Milestone 2 — Daemon "watch service" + event fanout** COMPLETED

- **Deliverables:**
  - Implemented `WatchService` struct with per-process watch tables for kqueue and FSEvents watches
  - Added path-to-FD mapping capability to support kqueue watches with path tracking
  - Implemented path prefix matching for FSEvents streams to determine event routing
  - Created event translation from `EventKind` to kqueue vnode flags and FSEvents event types
  - Integrated FsCore event subscription in `WatchServiceDaemon::subscribe_events()`
  - Added comprehensive unit tests for all routing logic and event translation
  - Created end-to-end integration test verifying full event pipeline from FsCore to routing

- **Implementation Details:**
  - **Watch Service Registry**: Extended `WatchService` to maintain `kqueue_watches: HashMap<(u32, u32, u64), KqueueWatchRegistration>` and `fsevents_watches: HashMap<(u32, u64), FSEventsWatchRegistration>`
  - **Kqueue Watch Registration**: Added path field to `KqueueWatchRegistration` struct for path-based routing
  - **Event Sink Implementation**: Created `WatchServiceEventSink` that implements `EventSink` trait and routes events to appropriate watchers
  - **Path Matching Logic**: Implemented exact path matching for kqueue watches and prefix matching for FSEvents streams
  - **Event Translation**: Mapped `EventKind::{Created, Removed, Modified, Renamed}` to kqueue vnode flags (`NOTE_WRITE`, `NOTE_DELETE`, `NOTE_RENAME`, etc.)
  - **Thread Safety**: Used `Arc<Mutex<...>>` for concurrent access to watch tables
  - **Event Coalescing**: Events are routed to all matching watchers without duplicate filtering

- **Key Source Files:**
  - `crates/agentfs-daemon/src/watch_service.rs`: Complete watch service implementation with registry, routing, and event sink
  - `crates/agentfs-interpose-e2e-tests/src/lib.rs`: End-to-end integration test for full event pipeline
  - `crates/agentfs-proto/src/messages.rs`: SSZ message types for watch registration and event broadcasting

- **Verification Results:**
  - [x] Spawn daemon with fake watches and synthetic `EventKind` resolves recipients correctly
  - [x] Created/Removed/Modified events hit matching kqueue fd paths
  - [x] Prefix matching works for FSEvents streams
  - [x] Event translation from `EventKind` to kqueue vnode flags is accurate
  - [x] Full event pipeline from FsCore subscription through daemon routing works
  - [x] Thread-safe concurrent access to watch tables
  - [x] No hits for unregistered targets (proper isolation)

# Milestone 3 — Kqueue "doorbell" channel (Option 4) COMPLETED

**Goal:** The shim owns each app's real kqueue, but the **daemon can notify it** by posting EVFILT_USER "doorbells" on that same kqueue (daemon receives the **kqueue fd via `SCM_RIGHTS`**), avoiding polling/timeouts.

- **Shim (on first `kqueue()` call)**
  1. Call real `kqueue()`; store `kq_fd`.
  2. Allocate a **reserved EVFILT_USER ident** for this kq: `ident = 0xAFFE00000000 | (shim_random_32())`. Keep this disjoint from app's space.
  3. `EV_SET(&kev, ident, EVFILT_USER, EV_ADD|EV_ENABLE, 0, 0, udata=shim_marker)`
  4. Send **`WatchDoorbell{kq_fd, ident, proc_pid}`** to daemon, passing `kq_fd` via `SCM_RIGHTS` over the existing UNIX control socket. (FD passing infra already used elsewhere.)

- **Shim (collision hygiene - during `kevent()` calls)**
  - Intercept `kevent()` calls and scan the changelist for `EV_ADD` operations on `EVFILT_USER` with our reserved ident range.
  - If collision detected: immediately `EV_DELETE` the current doorbell ident, allocate a new ident, re-register it, and send **`UpdateDoorbellIdent{old_ident, new_ident, proc_pid}`** to daemon.
  - Reason: kqueue events are unique by `(ident, filter)` on a given kqueue - collisions would cause undefined behavior.

- **Daemon**
  - Accepts the `kq_fd` and caches `{proc_pid → kq_fd, ident}`.
  - Handles `UpdateDoorbellIdent` messages by updating the cached ident for the process.
  - To wake the app's `kevent()` immediately, post:
    - `EV_SET(&kev, ident, EVFILT_USER, 0, NOTE_TRIGGER, data=payload_id, udata=NULL); kevent(kq_fd, &kev, 1, NULL, 0, NULL);`

  - `payload_id` indexes a **per-kqueue lock-free ring buffer** (next milestone) holding synthesized `struct kevent` entries to be merged by the shim.

- **Acceptance (end-to-end test)**
  - Tiny test app calls `kqueue()` and then `kevent()` with a long timeout; daemon posts a doorbell → app wakes with **one** EVFILT_USER event (correct `ident`), **no polling**.
  - Also prove **no collision**: the app registers its own EVFILT_USER `ident = 123`; daemon doorbell uses reserved range; both events can be delivered independently. (Hook ensures we don't tamper with non-vnode filters.)
  - **Collision hygiene**: test app registers EVFILT_USER with shim's doorbell ident → shim detects collision, deletes old doorbell, registers new ident, notifies daemon → daemon updates its cached ident → subsequent doorbells work with new ident.

**Milestone 3 — Kqueue "doorbell" channel (Option 4)** COMPLETED

- **Deliverables:**
  - Implemented kqueue interception in shim with doorbell ident allocation using reserved range `0xAFFE00000000 | random_32bit`
  - Added collision hygiene to detect and resolve EVFILT_USER ident conflicts during kevent() calls
  - Implemented SCM_RIGHTS file descriptor passing from shim to daemon for doorbell registration
  - Added UpdateDoorbellIdent and QueryDoorbellIdent message types to protocol
  - Created efficient doorbell ident lookup in daemon with dedicated HashMap for O(1) access
  - Implemented doorbell posting mechanism using NOTE_TRIGGER for immediate wakeup
  - Added comprehensive end-to-end test for collision hygiene verification

- **Implementation Details:**
  - **Shim Interception**: Modified `my_kqueue()` to allocate reserved doorbell ident and register EVFILT_USER event, then send WatchDoorbell message with FD via SCM_RIGHTS
  - **Collision Detection**: Added `my_kevent()` interception to scan changelist for EV_ADD operations on EVFILT_USER in reserved range, triggering collision resolution
  - **Atomic Doorbell Storage**: Replaced HashMap with single `AtomicU64` for current doorbell ident to simplify state management
  - **Protocol Extensions**: Added `UpdateDoorbellIdentRequest/Response` and `QueryDoorbellIdentRequest/Response` to `agentfs-proto` with proper SSZ serialization
  - **Daemon Handling**: Extended WatchService with doorbell ident tracking and update mechanisms, implemented efficient lookup methods
  - **Thread Safety**: Used atomic operations for doorbell ident access and Mutex for watch table modifications
  - **FD Passing**: Leveraged existing UNIX domain socket infrastructure for secure kqueue FD transfer between processes

- **Key Source Files:**
  - `crates/agentfs-interpose-shim/src/lib.rs`: Kqueue interception, doorbell registration, and collision hygiene logic
  - `crates/agentfs-daemon/src/watch_service.rs`: Doorbell ident tracking and posting mechanisms
  - `crates/agentfs-daemon/src/bin/agentfs-daemon.rs`: IPC message handling for doorbell operations
  - `crates/agentfs-proto/src/messages.rs`: New message types for doorbell communication
  - `crates/agentfs-interpose-e2e-tests/src/bin/test_helper.rs`: End-to-end collision hygiene test

- **Verification Results:**
  - [x] Tiny test app calls kqueue() and kevent() with long timeout; daemon posts doorbell → app wakes with one EVFILT_USER event (correct ident), no polling
  - [x] App registers EVFILT_USER ident=123; daemon doorbell uses reserved range; both events delivered independently
  - [x] Collision hygiene: test app registers EVFILT_USER with shim's doorbell ident → shim detects collision, deletes old doorbell, registers new ident, notifies daemon → daemon updates cached ident → subsequent doorbells work with new ident
  - [x] SCM_RIGHTS FD passing works correctly between shim and daemon processes
  - [x] Thread-safe atomic operations for doorbell ident management
  - [x] Efficient O(1) doorbell ident lookup in daemon

# Milestone 4 — Shim kevent() hook + injectable queue COMPLETED

**Goal:** Interpose `kevent*(…)` and splice **synthesized EVFILT_VNODE** entries into the result set whenever a doorbell fires, without disturbing unrelated filters.

- **Hook behavior** (public API only)
  1. Call the **real** `kevent` first to collect kernel events.
  2. If the result includes our **doorbell EVFILT_USER** ident, request the pending synthesized events from the AgentFS daemon per-kqueue buffer.
  3. Append those `struct kevent` entries to the user’s output buffer; adjust returned `nevents`.
  4. Pass through all non-EVFILT_VNODE events untouched; only add/modify EVFILT_VNODE for watched fds.

- **API details to list in code/README**
  - `struct kevent { uintptr_t ident; short filter; unsigned short flags; unsigned int fflags; intptr_t data; void *udata; }`
  - Flags we use: `EV_ADD|EV_ENABLE|EV_CLEAR|EV_DELETE` (never touch app’s other filters).
  - Vnode fflags we synthesize: `NOTE_DELETE|NOTE_WRITE|NOTE_EXTEND|NOTE_ATTRIB|NOTE_LINK|NOTE_RENAME|NOTE_REVOKE`.

- **Acceptance (end-to-end tests)**
  - ✅ App registers `EVFILT_VNODE` on fd for `/file.txt` (opened with `O_EVTONLY`), then sleeps in `kevent()`.
  - ✅ FsCore write/rename/unlink (issued via daemon on behalf of another proc) causes daemon to enqueue **matching EVFILT_VNODE**; daemon posts doorbell; shim merges; app receives correct flags in **one** call.
  - ✅ Ensure **unrelated filters** (timers, signals, sockets) pass through unchanged.
  - ✅ **COMPLETED**: Milestone 4 acceptance test implemented as `test_milestone_4_kevent_hook_injectable_queue` in `crates/agentfs-interpose-e2e-tests/src/lib.rs`.

**Milestone 4 — Shim kevent() hook + injectable queue** COMPLETED

- **Deliverables:**
  - Implemented full kevent() interception and event injection pipeline in the interpose shim
  - Added `WatchDrainEvents` protocol message for requesting pending synthesized events from daemon
  - Created per-kqueue event queuing system in daemon's WatchService with proper thread safety
  - Implemented event merging logic that preserves unrelated filters while injecting synthesized EVFILT_VNODE events
  - Added watcher table tracking in shim to manage EVFILT_VNODE registrations
  - Created comprehensive end-to-end test framework for verifying the complete event pipeline

- **Implementation Details:**
  - **Protocol Extension**: Added `WatchDrainEventsRequest/Response` SSZ messages to `agentfs-proto` for shim-daemon communication
  - **Event Queuing**: Extended `WatchService` with per-kqueue event storage using thread-safe queue structures
  - **Shim Interception**: Modified `my_kevent()` to detect doorbell events, request pending events, and merge them into results
  - **Event Synthesis**: Created `SynthesizedKevent` struct compatible with `libc::kevent` for seamless injection
  - **Watcher Tracking**: Added `WATCHER_TABLE` to track EVFILT_VNODE registrations during kevent() changelist processing
  - **Event Merging**: Implemented logic to append synthesized events to kernel events while preserving original event order and counts
  - **Thread Safety**: Used Mutex-protected HashMap for event queues with proper locking patterns

- **Key Source Files:**
  - `crates/agentfs-proto/src/messages.rs`: New WatchDrainEvents protocol messages and SynthesizedKevent struct
  - `crates/agentfs-daemon/src/watch_service.rs`: Event queuing system and draining logic
  - `crates/agentfs-daemon/src/bin/agentfs-daemon.rs`: IPC handling for WatchDrainEvents requests
  - `crates/agentfs-interpose-shim/src/lib.rs`: kevent() interception, event injection, and watcher table management
  - `crates/agentfs-interpose-e2e-tests/src/lib.rs`: End-to-end test implementation

- **Verification Results:**
  - [x] App registers EVFILT_VNODE on fd for test file (opened with O_EVTONLY), then sleeps in kevent()
  - [x] FsCore write/rename/unlink operations cause daemon to enqueue matching EVFILT_VNODE events
  - [x] Daemon posts doorbell which wakes shim's kevent() call
  - [x] Shim successfully merges synthesized events into application-visible results
  - [x] Unrelated filters (timers, signals, sockets) pass through unchanged
  - [x] Event injection preserves correct flag sets and occurs in one kevent() call
  - [x] Thread-safe event queuing and draining works correctly
  - [x] End-to-end test framework compiles and runs without errors

# Milestone 5 — Daemon synthesizer (FsCore → vnode flags) COMPLETED

**Goal:** Deterministic mapping of FsCore `EventKind` to per-watch **EVFILT_VNODE** tuples with coalescing.

- **Mapping**
  - Created(path) → `NOTE_WRITE` for parent dir watcher; (optional) `NOTE_LINK` for target if someone watches fd of the new file post-open.
  - Removed(path) → `NOTE_DELETE` on file; `NOTE_WRITE` on parent dir.
  - Modified(path) → `NOTE_WRITE|NOTE_EXTEND` (set both only when size changed).
  - Renamed{from,to} → `NOTE_RENAME` (deliver on fds that refer to either the old or new vnode path).

- **Coalescing & ordering**
  - Per fd, coalesce duplicate flags before enqueue.
  - Preserve **happens-before** within a single operation (rename → delete+create pairs for dirs require careful flagging).

- **Acceptance (daemon unit tests)**
  - Table-driven tests for each `EventKind` produce the expected `(fd, flags)` set.
  - Coalescing reduces bursts (e.g., write loop) to a minimal flag set.

**Milestone 5 — Daemon synthesizer (FsCore → vnode flags)** COMPLETED

- **Deliverables:**
  - Enhanced `event_to_vnode_flags()` method with proper file vs directory watcher handling
  - Added `is_directory` field to `KqueueWatchRegistration` for distinguishing watcher types
  - Implemented event coalescing per file descriptor in `enqueue_event()` method
  - Fixed deadlock in `post_doorbell()` by avoiding lock contention during routing
  - Added comprehensive unit tests covering all EventKind mappings and coalescing scenarios
  - Proper directory watcher support for parent path notifications

- **Implementation Details:**
  - **Event Mapping Logic**: Enhanced `event_to_vnode_flags()` to handle file vs directory watchers with correct NOTE\_\* flag assignment:
    - File watchers: Created → `NOTE_WRITE`, Removed → `NOTE_DELETE`, Modified → `NOTE_WRITE|NOTE_EXTEND`, Renamed → `NOTE_RENAME`
    - Directory watchers: Created/Removed/Renamed → `NOTE_WRITE`, Modified → `NOTE_ATTRIB`
  - **Directory Watcher Support**: Added `is_directory` boolean field to watch registrations and routing logic to handle parent directory notifications
  - **Event Coalescing**: Modified `enqueue_event()` to coalesce flags for the same (pid, kq_fd, fd) combination, preventing duplicate events
  - **Thread Safety**: Fixed deadlock issue in `post_doorbell()` by using separate doorbell_idents map instead of accessing watches during routing
  - **Path Relevance Checking**: Enhanced routing logic to properly determine when directory watchers are interested in child path events

- **Key Source Files:**
  - `crates/agentfs-daemon/src/watch_service.rs`: Enhanced event mapping, coalescing, and routing logic
  - `crates/agentfs-daemon/src/watch_service.rs` (tests): Comprehensive unit tests for all mapping scenarios

- **Verification Results:**
  - [x] All EventKind mappings produce correct vnode flags for file and directory watchers
  - [x] Event coalescing works correctly for multiple operations on same file descriptor
  - [x] Directory watchers receive appropriate events for child file changes
  - [x] Parent directory notifications work for file creation/deletion/renames
  - [x] Thread safety issues resolved (deadlock fix)
  - [x] All 25 unit tests passing including new comprehensive event mapping tests

# Milestone 6 — FSEvents interpose path (parallel path)

**Goal:** For apps using FSEvents, wrap callbacks and inject custom notifications consistent with overlay view (already spec’d).

- **Shim**
  - Interpose `FSEventStreamCreate*`, store original callback/context; **translate paths** at create time; inject/suppress in our replacement callback.
  - Schedule/Start/Stop/Invalidate remain pass-through, preserving runloop delivery.

- **Daemon**
  - Fan out FsCore events by **prefix** to active FSEvents streams and send `FsEventBroadcast` to shim, which converts to valid `FSEventStreamEvent` entries for the app.

- **Acceptance (end-to-end)**
  - App registers FSEvents on overlay path; FsCore mutates; app callback gets path-translated events in the right runloop context, alongside real events.

# Milestone 7 — Registration lifecycle & robustness

**Goal:** Hardening of register/unregister, app exit, daemon restarts.

- **Scenarios**
  - App closes watched fd → daemon removes that watch atomically.
  - App exits → shim informs daemon (or daemon detects socket close) → reclaim `kq_fd` and ring buffer.
  - Daemon restart → shim re-handshakes and re-sends registrations.

- **Acceptance (integration)**
  - Killing the test app or daemon does not deadlock; watchers self-heal or tear down cleanly.

# Milestone 8 — Negative & invariants matrix

**Goal:** Ensure we never “leak” backstore paths or mutate unrelated events. (Echoes your existing M24 negative matrix.)

- **Tests**
  - `realpath`/`F_GETPATH` seen by the target app always reflects overlay view, never backstore. (Regression guard while we attach fds.)
  - Injected events only for files the process _could_ see (respect whiteouts/permissions).

# Milestone 9 — Performance & load

**Goal:** Validate cost and behavior under pressure.

- **Tests (bench/regression)**
  - Burst 100k FsCore `Modified` on one file → expect O(1) wakeups via coalescing.
  - Many watches (e.g., 10k fds) → constant-factor overhead in shim hook.
  - Ensure zero unexpected wakeups: app sleeping in `kevent()` only wakes on doorbell or real events.

# Milestone 10 — Documentation & samples

**Goal:** First-class developer docs + examples.

- `agentfs-interpose-shim/README.md`: add a “File Monitoring” section with:
  - EVFILT_USER reservation scheme, `EV_SET` templates, and end-to-end flowchart.
  - Safety notes: “never touch non-vnode events,” “deliver on the caller’s thread.”

- Demo apps:
  - `kq_demo`: registers `EVFILT_VNODE`, prints events.
  - `fse_demo`: registers FSEvents stream, prints events.

---

## Automated Test Harness (how to wire this up)

- **Process orchestration**
  - Tests spawn:
    1. the daemon (real binary in test mode) with ephemeral UNIX socket,
    2. the target app (small helper) with `DYLD_INSERT_LIBRARIES=<shim>.dylib` and env pointing to the socket. (You already use this pattern.)

- **Assertions**
  - For kqueue: helper registers watch, then blocks in `kevent()`. Tests drive FsCore mutations through the daemon API. The helper prints a stable line per event; the parent test process reads stdout and asserts the exact **flag set & order** (no sleep/poll loops; doorbell guarantees wakeup).

- **CI profile**
  - Tag macOS-only tests with `#[cfg(target_os="macos")]`.
  - Provide an opt-in `MACOS_E2E=1` to enable heavier end-to-end suites on GitHub Actions macOS runners.

---

## API Cheat-Sheet (copy into docs)

- **kqueue / kevent (macOS / BSD)**

  ```c
  int kq = kqueue();
  struct kevent kev;
  EV_SET(&kev, ident, EVFILT_USER, EV_ADD | EV_ENABLE, 0, 0, udata);
  // Post from daemon (after receiving kq via SCM_RIGHTS):
  EV_SET(&kev, ident, EVFILT_USER, 0, NOTE_TRIGGER, payload_id, NULL);
  kevent(kq, &kev, 1, NULL, 0, NULL);
  ```

  - `struct kevent { uintptr_t ident; short filter; unsigned short flags; unsigned int fflags; intptr_t data; void *udata; }`
  - Vnode flags we synthesize: `NOTE_DELETE|NOTE_WRITE|NOTE_EXTEND|NOTE_ATTRIB|NOTE_LINK|NOTE_RENAME|NOTE_REVOKE`.
  - **Shim rule:** only intercept EVFILT_VNODE; pass others through.

- **FSEvents (public API)**
  - Interpose `FSEventStreamCreate*`, wrap callback; translate/forward events; preserve runloop/threading.

- **FsCore events**
  - `EventKind::{Created, Removed, Modified, Renamed, …}`, `EventSink::on_event(&EventKind)`.

---
