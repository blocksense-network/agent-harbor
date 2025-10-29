Great question. When FsCore is fully in-memory (no backing store), the kernel won’t produce real file system events. So we have to **bridge FsCore’s own change notifications to the APIs apps actually use** and make them look native. The high-level plan is:

- Turn on FsCore’s event stream and subscribe to it.
- Interpose the app’s monitoring APIs and **inject realistic events**:
  - **kqueue/kevent (EVFILT_VNODE)** for FD-level watches (and for Dispatch sources, which are built on kqueue).
  - **FSEvents** for directory-tree watchers (wrap the stream callback and deliver synthetic events on the app’s runloop).

Below is a practical, step-by-step implementation guide.

---

# 1) Enable and subscribe to FsCore events

1. **Enable event tracking** in `FsConfig` (use `InMemory` backstore here):

   ```rust
   FsConfig { track_events: true, backstore: BackstoreMode::InMemory, ..Default::default() }
   ```

   This configuration pattern is used throughout the tests and ensures the core publishes events.

2. **Subscribe to events** with an implementation of `EventSink::on_event(&EventKind)`. FsCore exposes a concise event model we can map to OS signals (Created/Removed/Modified/Renamed …).

> At this point you have a single place where every in-memory change is published, even though the kernel knows nothing about it.

---

# 2) Interpose the monitoring APIs (capture watchers)

We need to learn **what each app is watching** so we can route FsCore events to it.

## 2.1 kevent family (covers Dispatch sources too)

- **Interpose** `kqueue`, `kevent`, `kevent64`, `kevent_qos`.
- In the interposed `kevent(...)` examine the _change list_ arguments and **record registrations** for `EVFILT_VNODE` with `EV_ADD`/`EV_DELETE`. Keep a `WatcherRegistry` keyed by **(kq FD, watched FD)** with the flag mask the app requested (NOTE_WRITE, NOTE_DELETE, NOTE_RENAME, NOTE_ATTRIB…).
- Also interpose **`open`/`openat` (O_EVTONLY)**, `dup*`, and `close` to maintain a **fd→path (and path→fd set)** map; you’ll need this to route path-based FsCore events to FD-based watchers.

> Because GCD’s `DISPATCH_SOURCE_TYPE_VNODE` sits on top of kqueue, **covering kevent covers Dispatch automatically**.

### 2.2 FSEvents

- **Interpose** `FSEventStreamCreate*` to wrap the app’s callback/context; record the **root paths** the app requested. Also interpose `FSEventStreamScheduleWithRunLoop` and `FSEventStreamStart` to capture the **runloop/queue** where we must deliver.
- We won’t rely on the kernel’s fseventsd at all here; instead, we’ll **synthesize FSEvents** out of FsCore updates and dispatch them on the same runloop thread so delivery looks native.

---

# 3) Data structures

- `WatcherRegistry`:
  - By **kqueue FD → { watched FD → mask, isDir }**.
  - By **FSEventsStreamID → { roots: [Path], runloop: CFRunLoopRef, callback: FSEventsCallback }**.

- `FdIndex`:
  - **fd → canonical path** (update on `open/dup/close`).
  - **path → set<fd>** for fast reverse lookup (directory parents too).

- `PendingQueues`:
  - Per-**kqueue FD** ring buffer of `struct kevent` we plan to inject.
  - Per-**FSEvents stream** queue of batched path events.

> FsCore’s config and in-memory mode are already defined; we’re just adding these interpose-side indices.

---

# 4) Translating FsCore events → native signals

FsCore emits compact `EventKind`s: **Created / Removed / Modified / Renamed / …**.

Map them as follows:

- **Created {path}**
  - For **file FD watchers** on that file: enqueue `EVFILT_VNODE` with `NOTE_WRITE` (file contents/links changed).
  - For **directory FD watchers** on `parent(path)`: enqueue `NOTE_WRITE` (dir entries changed).
  - For **FSEvents**: emit a `kFSEventStreamEventFlagItemCreated` for paths under any watched root.

- **Removed {path}**
  - For file FD watchers: `NOTE_DELETE`.
  - For parent directory watchers: `NOTE_WRITE`.
  - FSEvents: `ItemRemoved`.

- **Modified {path}**
  - File FD watchers: `NOTE_WRITE`.
  - Directory FD watchers (if the node is a directory and metadata changed): optionally `NOTE_ATTRIB`.
  - FSEvents: `ItemModified` (or plain “changed” depending on the flags you choose).

- **Renamed {from, to}**
  - File FD watchers on the moved file’s FD: `NOTE_RENAME`.
  - Directory watchers on **src parent** and **dst parent**: `NOTE_WRITE`.
  - FSEvents: either two events (one under `from` tree, one under `to` tree) or a single rename with both paths depending on how you batch.

> Deliver everything **per process branch view**; if your overlay has whiteouts for this process, filter those paths before mapping (the hook’s mandate includes filtering).

---

# 5) Delivering events to **kevent** correctly (no kernel help)

The app may block in `kevent` waiting for events. We can’t ask the kernel to deliver ours, so the interposed function must manage the blocking:

1. **Replace the block with a cooperative wait** inside the shim:
   - If `PendingQueues[kq]` is **non-empty**:
     - **Do not** call the real `kevent`.
     - Copy up to `nevents` fabricated `struct kevent`s into `eventlist` and **return N** immediately.

   - Else (queue empty):
     - Call the real `kevent` with a **short timeout** (e.g., 10–20 ms) instead of infinite; on return, merge kernel events (for unrelated filters) with any synthetic ones that arrived meanwhile and return what fits.
     - If both remain empty and the app requested an infinite/long timeout, **loop**, but wait on a **condvar** that your FsCore subscriber signals whenever it enqueues a synthetic event. This preserves the blocking semantics without busy-waiting.

2. **Preserve non-VNODE events untouched** (sockets, timers, signals). Only inject/alter `EVFILT_VNODE`.

3. **Coalesce**: multiple quick writes/renames → one `NOTE_WRITE` per FD, unless the app asked for `NOTE_EXTEND/ATTRIB`-granularity. (Keep a per-FD bitmask that you OR as you batch, then flush as one event.)

4. **Respect app masks**: only emit flags the app registered for that FD.

> This “cooperative kevent” approach ensures Dispatch sources also wake on time since their internals call `kevent` too.

---

# 6) Delivering events to **FSEvents** correctly

1. When the app creates a stream, **wrap its callback** and remember `{roots, flags, latency, runloop}`.
2. Your FsCore subscriber, upon receiving an event, finds all streams whose **root is a prefix** of the path, aggregates paths for `latency` ms, and **posts back** using `CFRunLoopPerformBlock(stream.runloop, ^{ wrappedCallback(...synthetic batch...) })` followed by `CFRunLoopWakeUp(runloop)` to preserve delivery thread/ordering.
3. Emit flags consistent with what the app asked for (e.g., per-item flags like Created/Removed/Modified); keep generation numbers increasing per stream.

---

# 7) Routing logic (how an FsCore event finds its watchers)

Given an FsCore event on `/a/b/c.txt`:

- **Find file FD watchers**: look up `fd ∈ path→fd set` and check masks → enqueue kevents for each `(kq, fd)`.
- **Find directory watchers**:
  - Source parent `/a/b`: enqueue `NOTE_WRITE`.
  - Renames: also notify destination parent.

- **Find FSEvents streams**: any stream whose root prefixes `/a/b/c.txt` gets a path in its batch.
- **Per-process filters**: before all of the above, apply whiteout/branch visibility for the **calling process’ PID token** (FsCore tracks PIDs).

---

# 8) Edge cases & correctness

- **FD lifetime**: handle `close(fd)` (remove watcher), `dup` (copy watcher), `fstatat`-based opens, and directory FDs.
- **Directory semantics**: EVFILT_VNODE is **per vnode**. When a child is created/removed, you don’t get per-child events on a **directory FD**; you synthesize a **directory `NOTE_WRITE`** for its watchers, plus per-file flags for any direct file FD watchers (if they exist).
- **Attribute changes**: FsCore doesn’t expose an explicit attribute event; treat chmod/chown/xattr paths coming through your Core API as **`NOTE_ATTRIB`** in addition to `NOTE_WRITE` when appropriate. (Optionally extend `EventKind` to include `Attrib` later.)
- **Backpressure**: cap per-kqueue pending queue length; if over capacity, coalesce aggressively.
- **Threading**: FSEvents deliveries occur on the app’s scheduled runloop; kevent returns on the **calling thread**; keep it that way.

---

# 9) Pseudocode sketch

## FsCore subscriber → inject

```rust
fn on_event(evt: &EventKind) {
  let paths = affected_paths(evt); // includes parents for dirs
  for (path, action) in paths {
    // 1) kqueue
    for fd in fd_index.fds_for_path(&path) {
      for (kq, mask) in watchers.watchers_for_fd(fd) {
        if should_emit(mask, action) {
          pending[kq].push(make_kevent(fd, mask_for(action)));
        }
      }
    }
    // 2) FSEvents
    for stream in streams.covering(&path) {
      stream.queue.push((path.clone(), flags_for(action)));
      stream.maybe_flush_after_latency();
    }
  }
  condvar.notify_all(); // wake any blocked interposed kevent callers
}
```

### Interposed `kevent`

```c
int kevent_shim(int kq, const struct kevent *ch, int nch,
                struct kevent *out, int maxevents,
                const struct timespec *ts) {
  registry_apply_changes(kq, ch, nch);           // record EV_ADD/EV_DELETE for EVFILT_VNODE
  if (pending_has(kq))                           // synthetic events already waiting?
    return drain_pending(kq, out, maxevents);    // return without calling real kevent

  // Otherwise, wait cooperatively for either kernel or synthetic events
  for (;;) {
    int n = real_kevent(kq, NULL, 0, out, maxevents, short_timeout(ts));
    if (n > 0) return n + drain_into(out+n, maxevents-n, kq); // merge kernel + synthetic
    if (pending_has(kq)) return drain_pending_into(out, maxevents, kq);
    if (deadline_passed(ts)) return 0;
    condvar_wait(); // woken when FsCore subscriber enqueues something
  }
}
```

(Only alter `EVFILT_VNODE`; pass all other filters through untouched.)

---

# 10) Tests you should add

- **In-memory core, track_events=true**; app watches a file FD via `kevent`:
  - write → app receives `NOTE_WRITE`.

- **Directory watcher**:
  - create child → app receives `NOTE_WRITE` on dir FD.

- **Rename across directories**:
  - file FD watcher sees `NOTE_RENAME`; both dirs see `NOTE_WRITE`.

- **Dispatch source**:
  - same as above (proves the kqueue path covers Dispatch).

- **FSEvents stream**:
  - create/modify/remove under watched root → stream callback fires on the same runloop with batched paths.

---

# 11) Operational notes

- This works the same whether overlay/backstore is present or not; for the **pure in-memory scenario** just keep `BackstoreMode::InMemory` and `track_events: true`.
- Keep **Interpose** enabled (even with no backstore) so the shim is active.
- Remember: **filter by branch/whiteouts** before emitting—your kqueue/FSEvents hooks are expressly allowed to filter and inject for consistency with the per-process view.

---

If you’d like, I can sketch out concrete structs for `WatcherRegistry`, the fd/path index, and a minimal event-coalescer next.
