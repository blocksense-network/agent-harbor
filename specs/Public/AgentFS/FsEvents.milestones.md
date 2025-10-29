Here’s a concrete, test-driven development plan to implement the full “shim ↔ daemon ↔ FsCore” event pipeline (kqueue/FSEvents interception, daemon-driven EVFILT_USER doorbells, kernel-like EVFILT_VNODE synthesis, and FsCore event triggers). Each milestone ends with **fully automated** verification steps you can wire into `cargo test` on macOS runners.

# Milestone 0 — Groundwork: shared types, feature flags, crates

**Goal:** Create a clean skeleton so later milestones snap in without churn.

- **Crates/Modules**
  - `agentfs-core`: already defines event model; expose `EventKind`, `EventSink`, and a subscription API behind a feature `events` (enabled by default).
  - `agentfs-interpose-shim`: DYLD interposer that already handshakes with the daemon over a UNIX socket; we’ll add kqueue interception and event injection here.
  - `agentfs-daemon`: control plane + FsCore host; add “watch service” (registry + fanout + doorbells).
  - `agentfs-proto`: SSZ/JSON message types covering:
    - `WatchRegisterKqueue` (per kqueue fd + per-fd filter registrations)
    - `WatchRegisterFSEvents` (stream params)
    - `WatchUnregister`
    - `WatchDoorbell` (identify doorbell for a kqueue, see M3)
    - `FsEventBroadcast` (FsCore → daemon → shim)

- **Acceptance (automated)**
  - Build succeeds with the feature `events`.
  - Shim loads & handshakes (already covered in existing tests).

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

# Milestone 2 — Daemon “watch service” + event fanout

**Goal:** Central registry that knows which target-process watches (kqueue/FSEvents) are interested in which paths and fans out FsCore events.

- **Daemon responsibilities**
  - Maintain per-process **watch table**:
    - **Kqueue watches:** `<proc_pid, kq_id, fd, vnode_flags>` (derive path once per fd with `F_GETPATH` and refresh lazily on rename).
    - **FSEvents watches:** `<proc_pid, stream_id, path_prefixes, flags>` (high-level).

  - Subscribe to FsCore (M1) and translate `EventKind` → abstract watch hits (path prefix & equality checks).

- **Acceptance (integration tests; no shim yet)**
  - Spawn daemon with **fake watches** and feed it synthetic `EventKind`; it resolves recipients correctly:
    - Created/Removed/Modified hits matching kqueue fd path.
    - Prefix matching for FSEvents streams (later used in M6).

  - Verify no hits for unregistered or whiteout-hidden targets (uses existing overlay/whiteout semantics in core).

# Milestone 3 — Kqueue “doorbell” channel (Option 4)

**Goal:** The shim owns each app’s real kqueue, but the **daemon can notify it** by posting EVFILT_USER “doorbells” on that same kqueue (daemon receives the **kqueue fd via `SCM_RIGHTS`**), avoiding polling/timeouts.

- **Shim (on first `kqueue()` call)**
  1. Call real `kqueue()`; store `kq_fd`.
  2. Allocate a **reserved EVFILT_USER ident** for this kq: `ident = 0xAFFE00000000 | (shim_random_32())`. Keep this disjoint from app’s space.
  3. `EV_SET(&kev, ident, EVFILT_USER, EV_ADD|EV_ENABLE, 0, 0, udata=shim_marker)`
  4. Send **`WatchDoorbell{kq_fd, ident, proc_pid}`** to daemon, passing `kq_fd` via `SCM_RIGHTS` over the existing UNIX control socket. (FD passing infra already used elsewhere.)

- **Daemon**
  - Accepts the `kq_fd` and caches `{proc_pid → kq_fd, ident}`.
  - To wake the app’s `kevent()` immediately, post:
    - `EV_SET(&kev, ident, EVFILT_USER, 0, NOTE_TRIGGER, data=payload_id, udata=NULL); kevent(kq_fd, &kev, 1, NULL, 0, NULL);`

  - `payload_id` indexes a **per-kqueue lock-free ring buffer** (next milestone) holding synthesized `struct kevent` entries to be merged by the shim.

- **Acceptance (end-to-end test)**
  - Tiny test app calls `kqueue()` and then `kevent()` with a long timeout; daemon posts a doorbell → app wakes with **one** EVFILT_USER event (correct `ident`), **no polling**.
  - Also prove **no collision**: the app registers its own EVFILT_USER `ident = 123`; daemon doorbell uses reserved range; both events can be delivered independently. (Hook ensures we don’t tamper with non-vnode filters.)

# Milestone 4 — Shim kevent() hook + injectable queue

**Goal:** Interpose `kevent*(…)` and splice **synthesized EVFILT_VNODE** entries into the result set whenever a doorbell fires, without disturbing unrelated filters.

- **Hook behavior** (public API only)
  1. Call the **real** `kevent` first to collect kernel events.
  2. If the result includes our **doorbell EVFILT_USER** ident, **pop** pending synthesized events from the per-kqueue **SPSC ring buffer** (shared with daemon via control socket messages, not shared memory).
  3. Append those `struct kevent` entries to the user’s output buffer; adjust returned `nevents`.
  4. Pass through all non-EVFILT_VNODE events untouched; only add/modify EVFILT_VNODE for watched fds.

- **API details to list in code/README**
  - `struct kevent { uintptr_t ident; short filter; unsigned short flags; unsigned int fflags; intptr_t data; void *udata; }`
  - Flags we use: `EV_ADD|EV_ENABLE|EV_CLEAR|EV_DELETE` (never touch app’s other filters).
  - Vnode fflags we synthesize: `NOTE_DELETE|NOTE_WRITE|NOTE_EXTEND|NOTE_ATTRIB|NOTE_LINK|NOTE_RENAME|NOTE_REVOKE`.

- **Acceptance (end-to-end tests)**
  - App registers `EVFILT_VNODE` on fd for `/file.txt` (opened with `O_EVTONLY`), then sleeps in `kevent()`.
  - FsCore write/rename/unlink (issued via daemon on behalf of another proc) causes daemon to enqueue **matching EVFILT_VNODE**; daemon posts doorbell; shim merges; app receives correct flags in **one** call.
  - Ensure **unrelated filters** (timers, signals, sockets) pass through unchanged.

# Milestone 5 — Daemon synthesizer (FsCore → vnode flags)

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
