Below is a precise, end‑to‑end plan for **implementing FsCore event triggers** and wiring them into the shim⇄daemon “doorbell” flow you chose (daemon holds the app’s kqueue FD and posts `EVFILT_USER` wakeups; the shim fabricates `EVFILT_VNODE` results). I’m assuming Rust for FsCore/daemon and C for the doorbell post; all API surfaces and call sites are spelled out.

---

## 0) What FsCore already exposes (baseline)

- **Event types & sink trait** — FsCore defines `EventKind` and `EventSink` (shown below). We’ll use these as the _only_ way the daemon learns about filesystem changes.

```rust
/// Event kinds for filesystem change notifications
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Created { path: String },
    Removed { path: String },
    Modified { path: String },
    Renamed { from: String, to: String },
    BranchCreated { id: BranchId, name: Option<String> },
    SnapshotCreated { id: SnapshotId, name: Option<String> },
}

/// Event sink trait for receiving filesystem change notifications
pub trait EventSink: Send + Sync {
    fn on_event(&self, evt: &EventKind);
}
```

- **Config switches & subscription** — Events only flow when `FsConfig.track_events == true`; FsCore exposes `subscribe_events(...) -> SubscriptionId` and `unsubscribe_events(...)`. (Tests assert no events when tracking is off.)

---

## 1) Event dispatcher inside FsCore

Add a small, lock‑free(ish) dispatcher that FsCore calls after each successful state‑changing operation.

### 1.1 Types & wiring

```rust
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct EventBus {
    subs: RwLock<HashMap<SubscriptionId, Arc<dyn EventSink>>>,
    next_id: AtomicU64,
}

impl EventBus {
    pub fn new() -> Self {
        Self { subs: RwLock::new(HashMap::new()), next_id: AtomicU64::new(1) }
    }

    pub fn subscribe(&self, s: Arc<dyn EventSink>) -> SubscriptionId {
        let id = SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.subs.write().unwrap().insert(id, s);
        id
    }

    pub fn unsubscribe(&self, id: SubscriptionId) {
        self.subs.write().unwrap().remove(&id);
    }

    pub fn emit(&self, evt: &EventKind) {
        // snapshot to avoid holding the lock during user callbacks
        let subs: Vec<Arc<dyn EventSink>> =
            self.subs.read().unwrap().values().cloned().collect();
        for s in subs {
            // fire-and-forget; sinks must be fast / non-blocking
            s.on_event(evt);
        }
    }
}
```

Integrate into FsCore:

```rust
pub struct FsCore {
    // ...
    events: Option<EventBus>, // present iff config.track_events==true
}

impl FsCore {
    pub fn subscribe_events(&self, sink: Arc<dyn EventSink>) -> Result<SubscriptionId, FsError> {
        self.events
            .as_ref()
            .ok_or(FsError::EventsDisabled)?
            .pipe(|bus| Ok(bus.subscribe(sink)))
    }

    pub fn unsubscribe_events(&self, id: SubscriptionId) -> Result<(), FsError> {
        self.events
            .as_ref()
            .ok_or(FsError::EventsDisabled)?
            .pipe(|bus| Ok(bus.unsubscribe(id)))
    }
}
```

> This matches current usage in tests (`subscribe_events` / `unsubscribe_events`) and the **events‑off** behavior when `track_events=false`.

---

## 2) Where to **trigger** events in FsCore

Emit _after_ the in‑memory mutation commits successfully (so observers never see “phantoms”). The mapping below keeps things minimal and sufficient for kqueue synthesis:

| FsCore operation                   | Triggered `EventKind`  | Notes                              |
| ---------------------------------- | ---------------------- | ---------------------------------- |
| `create(path)` (file, symlink)     | `Created { path }`     | After node insertion.              |
| `mkdir(path)`                      | `Created { path }`     | Directory creation.                |
| `unlink(path)`                     | `Removed { path }`     | After directory entry removal.     |
| `rmdir(path)`                      | `Removed { path }`     | Directory removal.                 |
| `write(h, ..)` (content change)    | `Modified { path }`    | Only if bytes changed.             |
| `truncate(path, new_len)`          | `Modified { path }`    | Treat size change as modified.     |
| `chmod/chown/utimes/xattr set/del` | `Modified { path }`    | Metadata change → Modified.        |
| `rename(from, to)`                 | `Renamed { from, to }` | Emit once per successful move.     |
| `link(newpath, oldpath)`           | `Created { newpath }`  | New hardlink appears at `newpath`. |
| `snapshot_create(..)`              | `SnapshotCreated{..}`  | Already in API.                    |
| `branch_create(..)`                | `BranchCreated{..}`    | Already in API.                    |

> FsCore’s tests drive many of these ops already; enabling `track_events: true` in the config used by tests follows the same pattern.

**Call site pattern** (example for `create`):

```rust
pub fn create(&self, pid: &PID, path: &Path, opts: &OpenOptions) -> FsResult<HandleId> {
    let h = self.do_create(pid, path, opts)?;      // commit mutation
    if let Some(bus) = &self.events {              // trigger
        bus.emit(&EventKind::Created { path: path.to_string_lossy().into_owned() });
    }
    Ok(h)
}
```

Repeat this “commit then emit” pattern for the other operations.

---

## 3) Daemon subscribes and becomes the **bridge**

The AgentFS daemon implements `EventSink` and subscribes once at startup (or per app‑session). Its `on_event` writes a full record into a **ring buffer** (or socket) and posts a **doorbell** to the app’s kqueue FD via `EVFILT_USER NOTE_TRIGGER`.

> The daemon process and control socket are already part of your interpose harness; the README shows the basic shim handshake and socket env var.

### 3.1 Event record (daemon ⇢ shim)

Keep it generic; the shim knows which FDs are being watched.

```rust
#[repr(C)]
pub struct AgentFsEvent {
    pub seqno: u64,        // monotonically increasing
    pub ts_nanos: u64,     // optional
    pub kind: u32,         // 0=Created,1=Removed,2=Modified,3=Renamed
    pub path_len: u32,
    pub aux_len: u32,      // for Renamed: len of 'to' path
    // [path bytes][aux bytes]
}
```

### 3.2 Mapping helper inside the daemon (optional precompute)

You may optionally add a cached **kqueue/vnode mask hint** to the record to save work in the shim:

```rust
fn vnode_mask_hint(evt: &EventKind) -> u32 {
    use EventKind::*;
    match evt {
        Created{..}  => NOTE_WRITE,               // parent dir entry set changed
        Removed{..}  => NOTE_DELETE | NOTE_WRITE, // file went away; parent changed
        Modified{..} => NOTE_WRITE,
        Renamed{..}  => NOTE_RENAME | NOTE_WRITE, // file + parents
        _ => 0,
    }
}
```

_(The actual selection/FD routing remains the shim’s job; the hook doc states we only alter/inject `EVFILT_VNODE` and pass others through.)_

### 3.3 `EventSink` impl in the daemon

```rust
use std::sync::atomic::{AtomicU64, Ordering};

struct DaemonSink {
    seq: AtomicU64,
    // per-app session state (ring writer, kqueue fd, doorbell ident, etc.)
    sessions: SessionRouter,
}

impl EventSink for DaemonSink {
    fn on_event(&self, evt: &EventKind) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let rec = encode_record(seq, evt);               // write to ring (shared mem or socket)
        self.sessions.broadcast(|sess| {
            sess.ring.write(&rec);
            post_doorbell(sess.kqueue_fd, sess.doorbell_ident, seq);
        });
    }
}
```

> The daemon code base already includes a real AgentFS daemon skeleton and a UNIX socket accept loop; integrate this sink alongside the existing control‑plane handlers.

### 3.4 Posting the **doorbell** (EVFILT_USER)

```c
#include <sys/event.h>
#include <sys/time.h>
#include <stdint.h>

void post_doorbell(int kq_fd, uint64_t doorbell_ident, uint64_t seqno) {
    struct kevent kev;
    EV_SET(&kev,
           (uintptr_t)doorbell_ident,
           EVFILT_USER,
           0 /* no EV_ADD here; it was added by the shim */,
           NOTE_TRIGGER | (uint32_t)(seqno & 0x00FFFFFFu),
           0,
           NULL);
    (void)kevent(kq_fd, &kev, 1, NULL, 0, NULL);
}
```

_(The shim registered the EVFILT_USER once when the kqueue was created and passed the kqueue FD to the daemon via `SCM_RIGHTS`; described earlier in the plan you approved.)_

---

## 4) Shim side (how your triggers surface to the app)

> The shim’s kevent hook filters/injects only `EVFILT_VNODE`, leaving all other filters intact, which is exactly what the hooking note prescribes.

**Flow inside interposed `kevent()`** (high level):

1. Call the **real** `kevent()` first to collect genuine kernel events.
2. If a doorbell fired (you can track this via a small “new data” flag near the ring), pull all new `AgentFsEvent` records.
3. For each record, map to one or more **synthetic `struct kevent`** entries:
   - Route by the app’s **watch table** (you already track `EV_ADD EVFILT_VNODE` registrations in the shim).
   - Emit the correct `NOTE_*` bits for each watched FD (created/removed/modified/renamed, plus parent dir `NOTE_WRITE` as appropriate).

4. Append those synthetic entries to the kernel batch and return the merged count.

> The hook design asserts we return on the **calling thread** and that GCD (DISPATCH_SOURCE_TYPE_VNODE) is automatically covered because it sits on kqueue—no extra APIs needed.

---

## 5) Routing, filtering, and correctness details

- **Canonicalization & case** — Normalize event paths per `FsConfig.case_sensitivity` before emitting, so reverse lookups (`path → watched fds`) in the shim behave predictably.

- **Branch/process view** — FsCore operations are invoked with a `PID` argument (tests show this), so FsCore knows the actor’s branch. If you need per‑branch filtering, wrap `EventKind` in an **envelope** that includes the actor PID/branch and emit that; the shim already scopes what it injects to the **current process** (interposed into that process), and your docs instruct filtering by whiteouts/branch.

- **Backpressure** — The event dispatcher should never block the critical path. If a sink (daemon) is slow, either:
  - write records to a bounded MPSC and drop/coalesce when full, or
  - have `on_event` enqueue and return, with a worker thread doing ring writes + doorbells.

- **Coalescing** — Keep FsCore events **fine-grained and immediate**. If you need burst coalescing (e.g., many writes), _do it in the daemon or shim_ before fabricating `EVFILT_VNODE`.

- **Track‑events off** — Leave everything inert when `track_events=false`. Tests already cover the “no events when disabled” case.

---

## 6) Minimal implementation checklist (with exact API calls to add)

### In **FsCore**

1. **Config gate**
   - Ensure `FsConfig { track_events: true, backstore: BackstoreMode::InMemory, .. }` in the profile you use during interpose.

2. **Embed dispatcher**
   - `events: Option<EventBus>` in `FsCore`. Initialize to `Some(EventBus::new())` when `track_events` is true; `None` otherwise.

3. **Public API**
   - `fn subscribe_events(&self, sink: Arc<dyn EventSink>) -> Result<SubscriptionId, FsError>`
   - `fn unsubscribe_events(&self, id: SubscriptionId) -> Result<(), FsError>`
     (These exist per tests; just point them to your `EventBus`.)

4. **Trigger points**
   - After **every successful, state‑changing** operation (see table in §2), call `events.emit(&EventKind::… )`.

### In **AgentFS daemon**

1. **Implement `EventSink`** and call `core.subscribe_events(Arc::new(DaemonSink{…}))` at startup.

2. **Per‑app session**
   - On shim handshake, receive: **kqueue FD** (via `SCM_RIGHTS`), **doorbell ident**, and ring handle/name. (Your interpose README shows the control socket pattern used for such handshakes.)

3. **on_event**
   - Encode event → ring record; post the doorbell using `kevent(kq_fd, EVFILT_USER/NOTE_TRIGGER, …)` as shown in §3.4.

### In **shim**

- Already covered by your hooking plan: intercept `kevent()` (and variants), maintain a watch table for `EVFILT_VNODE`, and **inject** synthetic results for FsCore events while passing non‑vnode filters through unchanged.

---

## 7) Test matrix you should add (quick references)

- **Events disabled**: `track_events=false`; subscribe→create→expect **0** events. (This test already exists.)
- **Basic lifecycle**: create/modify/rename/remove a file in a branch with `track_events=true`; verify daemon receives 4 records in order and the app (via shim) sees the corresponding `NOTE_*`.
- **Directory semantics**: mkdir + create child → app’s **directory FD** watcher gets `NOTE_WRITE`.
- **Rename across parents**: both old and new parent watchers get `NOTE_WRITE`; file watchers get `NOTE_RENAME`.
- **GCD dispatch source**: app uses `DISPATCH_SOURCE_TYPE_VNODE`; verify handler fires (covered by kevent path).

---

### Why this meets your requirements

- **All event triggers originate in FsCore after committed mutations**, so there’s no mismatch between reported and actual state.
- **No timeouts**: the daemon doorbells the app’s kqueue immediately; the shim merges records into the very `kevent()` the app is waiting in.
- **No API surprises**: we only use existing FsCore surfaces (`EventKind`, `EventSink`, `subscribe_events`) and the hooking guidance you already set for macOS event APIs (inject `EVFILT_VNODE`, preserve others).

If you want, I can supply a small patch set (FsCore dispatcher + DaemonSink + doorbell sender) that compiles against your current crates layout, plus a tiny e2e that shows a `rename` flowing from FsCore → daemon ring → doorbell → shim‑fabricated `EVFILT_VNODE`.
