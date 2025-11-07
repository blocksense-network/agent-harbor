# AgentFS “doorbell” design (shim + daemon) for apps using kqueue

Below is a precise, end-to-end plan for your chosen approach:

**When an app creates/uses a kqueue for filesystem monitoring, the shim:**

1. captures the kqueue FD,
2. passes a duplicate of that FD to the AgentFS daemon over a Unix-domain socket (`SCM_RIGHTS`), and
3. tells the daemon which **doorbell ident** to use for **EVFILT_USER** triggers.

The daemon performs the _real_ monitoring against FsCore (your in-memory overlay), enqueues full event records in an out-of-band channel (shared memory or the control socket), and **posts EVFILT_USER NOTE_TRIGGER** to the app’s kqueue using the passed FD. The shim’s interposed `kevent()` then reads those event records and **fabricates kernel-like `EVFILT_VNODE` events** that match the app’s registrations. This keeps the app’s code entirely unchanged.

---

## 1) API primer (all key pieces shown)

**kqueue / kevent** (create queue, register filters, wait for events)

```c
#include <sys/types.h>
#include <sys/event.h>
#include <sys/time.h>

int kqueue(void);

int kevent(int kq,
           const struct kevent *changelist, int nchanges,
           struct kevent *eventlist,    int nevents,
           const struct timespec *timeout);

/* helper to fill struct kevent */
EV_SET(&kev, ident, filter, flags, fflags, data, udata);

/* struct returned/consumed by kevent */
struct kevent {
    uintptr_t  ident;
    short      filter;
    u_short    flags;   /* EV_ADD, EV_DELETE, EV_CLEAR, EV_ERROR, ... */
    u_int      fflags;  /* filter-specific bits */
    int64_t    data;    /* filter-specific payload */
    void      *udata;   /* opaque cookie, returned unchanged */
};
```

- **Uniqueness**: within a single kqueue, an event key is **(ident, filter)** (unless macOS’s QoS API opts into including `udata`). ([Manual Pages][1])
- **Change semantics**: _all_ changes in `changelist` are applied **before** pending events are read. This guarantees your doorbell or registrations are visible to the same `kevent()` call. ([Apple Developer][2])

**Vnode filter & flags** (what apps typically register for)

```c
/* filter */   EVFILT_VNODE
/* fflags */   NOTE_DELETE | NOTE_WRITE | NOTE_EXTEND | NOTE_ATTRIB
               | NOTE_LINK | NOTE_RENAME | NOTE_REVOKE
```

These are the standard “file changed” bits apps use with `EVFILT_VNODE`. ([Manual Pages][1])

**User filter (our doorbell)**
`EVFILT_USER` lets user space trigger events. You set `NOTE_TRIGGER` to fire; the **low 24 bits** of `fflags` carry your tiny token (we’ll use it as a ring index / seqno). Helpers `NOTE_FFAND/FFOR/FFCOPY` can modify stored flags. ([Debian Manpages][3])

**Passing the app’s kqueue FD to the daemon (SCM_RIGHTS)**
Use Unix-domain sockets with `sendmsg/recvmsg` and a `cmsghdr` (`cmsg_level=SOL_SOCKET`, `cmsg_type=SCM_RIGHTS`) to transfer the **kqueue FD** to the daemon process. ([X-CMD][4])

**Why we only fabricate `EVFILT_VNODE`**
GCD `DISPATCH_SOURCE_TYPE_VNODE` is backed by kqueue; apps using GCD still consume `kevent()` results under the hood, so intercepting/augmenting `kevent()` is sufficient. ([Apple Developer][5])
(Your internal hook plan already calls out “filter/modify/inject `EVFILT_VNODE` events; pass others through.” )

---

## 2) High-level architecture

1. **Shim (in the app process)**
   - Hooks `kqueue()` and `kevent()` (and optionally `open(O_EVTONLY)` or `fcntl(F_GETPATH)` to map watched FDs → paths).
   - For each new kqueue:
     - picks a random 64-bit **doorbell ident** for `EVFILT_USER`,
     - **registers** that doorbell on the kqueue,
     - **dup()**s the kqueue FD and **sends it to the daemon via `SCM_RIGHTS`** along with the doorbell ident.

   - Tracks the app’s **`EVFILT_VNODE` registrations** (from `EV_ADD`) so it knows what to synthesize. (Matches your design doc.)

2. **AgentFS daemon**
   - Receives the kqueue FD and doorbell ident for this app session.
   - Subscribes to **FsCore** event stream (enable `track_events`), regardless of whether the backstore is in-memory; event records describe overlay changes.
   - Writes full event records to a per-session **ring buffer** (shared memory or the control socket), and on each arrival **posts `EVFILT_USER | NOTE_TRIGGER`** to the app’s kqueue **using the received FD**.

3. **Shim’s interposed `kevent()` (delivery)**
   - Calls the **real** `kevent()` first (to gather true kernel events).
   - Drains any pending **AgentFS ring records** (woken by the doorbell).
   - For each record, **fabricates `EVFILT_VNODE` events** that match the app’s registered fflags and FDs, merges them with the real events, and returns a single coherent batch. (As your doc mandates: only adjust vnode events; preserve everything else.)

---

## 3) Exact control flow & data structures

### 3.1 Shim: intercept creation & export the kqueue FD

**Hooked `kqueue()`**

- Call the real `kqueue()` → `kq_fd`.

- Choose random `uint64_t doorbell_ident;`

- Register doorbell once:

  ```c
  struct kevent kev;
  EV_SET(&kev, doorbell_ident, EVFILT_USER, EV_ADD|EV_CLEAR, 0, 0, NULL);
  kevent(kq_fd, &kev, 1, NULL, 0, NULL);
  ```

  (Adds a user event keyed by our ident to this **kqueue**.) ([Debian Manpages][3])

- Create a persistent UDS connection to the daemon (see §5), then **send the kqueue FD**:

  ```c
  /* sendmsg() with one CMSG of type SCM_RIGHTS carrying kq_fd */
  ```

  (Make sure to include at least one payload byte for portability.) ([Man Pages][6])

- Send a small bootstrap message on the same socket: `{ doorbell_ident, ring_name, ... }`.

**Collision hygiene (your chosen “option 4”)**
If the app later registers `EVFILT_USER` with our ident, our shim sees that `EV_ADD` in the intercepted `kevent()` change list; immediately `EV_DELETE` our doorbell and re-register a **new** ident; notify the daemon via the control socket. **Reason**: kevents are unique by `(ident, filter)` on a given kqueue. ([Manual Pages][1])

### 3.2 Shim: track what the app is watching

In your **interposed `kevent()`**, whenever you see **`EV_ADD` with `filter==EVFILT_VNODE`**, update a per-kqueue table:

```text
watch_table[kq_fd][watched_fd] = {
  requested_fflags, /* NOTE_* mask the app asked for */
  options_flags     /* EV_CLEAR, EV_ONESHOT, etc., for fidelity */
}
```

This mirrors the plan in your “Hooking … macOS” note (filter/modify/inject vnode events, leave others untouched).

(Optionally hook `open(..., O_EVTONLY)` or use `fcntl(F_GETPATH)` to map `watched_fd → path`, which helps correlate FsCore path events to FDs.)

### 3.3 Daemon: FsCore subscription & posting the doorbell

**FsCore** should be created with event tracking enabled and (for your target here) **`BackstoreMode::InMemory`** so the overlay exists without a host backing store.

Daemon loop per connection:

1. **recvmsg** → obtain the app’s **kqueue FD**. Store alongside `doorbell_ident`.
2. Create or attach a **ring buffer** (shared memory or keep using the socket) for event records.
3. Subscribe to FsCore’s event channel; on each FsCore event:
   - Encode `{type, path, ino, mask, ts, seqno, …}` and push to ring.
   - Compute a 24-bit token (e.g., `seqno & 0xFFFFFF`).
   - **Post doorbell**:

     ```c
     struct kevent kev;
     EV_SET(&kev, doorbell_ident, EVFILT_USER, 0,
            NOTE_TRIGGER | (token & NOTE_FFLAGSMASK), 0, NULL);
     kevent(app_kq_fd, &kev, 1, NULL, 0, NULL);
     ```

     (This wakes the app thread blocked in `kevent()`; the token guides the shim to the new records.) ([Debian Manpages][3])

> Important: the daemon must **never** read from `eventlist` on this kqueue—only submit the one-element changelist above—so it doesn’t steal the app’s events.

### 3.4 Shim: synthesize kernel-like `EVFILT_VNODE` results

**Hooked `kevent()` wait path**:

1. Call the real `kevent(kq, in, nin, out, nout, timeout)` first.
2. Drain ring records with `seqno > last_seen`. For **each** record:
   - Map event **path** back to any **watched FDs** in `watch_table[kq]` (or to all if you don’t track by path).
   - Convert FsCore event → **vnode mask** (`NOTE_WRITE`, `NOTE_ATTRIB`, etc.). ([Manual Pages][1])
   - If `(requested_fflags & produced_mask) != 0`, **append**:

     ```c
     EV_SET(&out[i++], watched_fd, EVFILT_VNODE, 0 /* no EV_ADD here */,
            produced_mask, 0 /* data */, /*udata*/ NULL);
     ```

3. Return the merged count. (Because “changelist is applied before reading,” any doorbell posted before your call is visible immediately.) ([Apple Developer][2])

---

## 4) Wire protocol & data plane

**Control socket (shim ⇄ daemon)** (simple, text or SSZ/CBOR):

```json
// bootstrap (shim→daemon)
{
  "cmd": "register_kqueue",
  "doorbell_ident": "<u64>",
  "ring_name": "/agentfs_ring_1234",
  "pid": 1234
}
```

(Your interpose component already uses a persistent control socket with env vars like `AGENTFS_INTERPOSE_SOCKET`—reusing that keeps things simple.)

**Ring buffer** (daemon→shim, arbitrary payload size)

- Shared-memory via `shm_open`/`ftruncate`/`mmap` or just reuse the socket.
- Record:

```c
struct AgentFsEvent {
  uint64_t seqno;
  uint32_t kind;          /* create, write, truncate, attrib, rename, delete */
  uint32_t vnode_mask;    /* precomputed NOTE_* to speed shim path */
  uint64_t ino;           /* if you track it */
  uint64_t ts_nanos;
  /* variable: path bytes or a handle/id */
};
```

- The daemon posts `EVFILT_USER NOTE_TRIGGER | (seqno & 0xFFFFFF)`; shim reads until `last_seen == seqno`.

---

## 5) Exact `SCM_RIGHTS` snippet (send the kqueue FD)

Sender (shim):

```c
int sock = /* connected AF_UNIX SOCK_STREAM */;
int fd_to_send = kq_fd;

struct msghdr msg = {0};
char buf = 'F';  /* portable: include 1 data byte */
struct iovec io = { .iov_base = &buf, .iov_len = 1 };
msg.msg_iov = &io; msg.msg_iovlen = 1;

char cmsgbuf[CMSG_SPACE(sizeof(int))];
msg.msg_control = cmsgbuf;
msg.msg_controllen = sizeof(cmsgbuf);

struct cmsghdr *cmsg = CMSG_FIRSTHDR(&msg);
cmsg->cmsg_level = SOL_SOCKET;
cmsg->cmsg_type  = SCM_RIGHTS;
cmsg->cmsg_len   = CMSG_LEN(sizeof(int));

memcpy(CMSG_DATA(cmsg), &fd_to_send, sizeof(int));

sendmsg(sock, &msg, 0);
```

Receiver (daemon) calls `recvmsg()` and extracts the FD from `CMSG_DATA`. ([X-CMD][4])

---

## 6) Edge cases & lifecycle

- **Multiple kqueues per process**: repeat the register/send flow for each `kqueue()` you see; maintain per-kqueue state (doorbell ident, watch table).
- **Doorbell ident collisions**: if shim later observes the app adding `EVFILT_USER` with our ident, rotate ident (delete+add) and send an **update** message. Uniqueness is per (ident, filter) **per kqueue**. ([Manual Pages][1])
- **kqueue close()**: interpose `close()`. When the app closes `kq_fd`, tell the daemon to close its copy; otherwise you keep the queue alive unintentionally (kqueues are FDs). ([Apple Developer][2])
- **Non-filesystem filters**: your shim must pass through everything not `EVFILT_VNODE` unchanged (timers, sockets, signals). (Matches your doc’s guidance.)
- **Apps using GCD**: no extra work—your `kevent()` interpose covers them.

---

## 7) Minimal “ingredients list” (for implementers)

- **Headers**: `<sys/event.h>`, `<sys/time.h>`, `<sys/socket.h>`, `<sys/un.h>`, `<fcntl.h>`.
- **Must-know constants**:
  - Filters: `EVFILT_VNODE`, `EVFILT_USER`.
  - Flags: `EV_ADD`, `EV_DELETE`, `EV_ENABLE`, `EV_DISABLE`, `EV_CLEAR`, `EV_ONESHOT`, `EV_ERROR`. ([Manual Pages][1])
  - Vnode notes: `NOTE_DELETE`, `NOTE_WRITE`, `NOTE_EXTEND`, `NOTE_ATTRIB`, `NOTE_LINK`, `NOTE_RENAME`, `NOTE_REVOKE`. ([Manual Pages][1])
  - User notes: `NOTE_TRIGGER`, `NOTE_FFAND`, `NOTE_FFOR`, `NOTE_FFCOPY`, `NOTE_FFLAGSMASK` (low 24 bits). ([Debian Manpages][3])

---

## 8) Why this works (and stays indistinguishable)

- The app only registered `EVFILT_VNODE` filters; it never needs to know about `EVFILT_USER`. We use `EVFILT_USER` purely as an internal wakeup **on the app’s own kqueue** (we have a valid FD for it). ([Debian Manpages][3])
- Because `kevent()` applies changes _before_ reading events, a doorbell arriving during your call is visible in the same round, so **no polling/timeout** is needed. ([Apple Developer][2])
- Your existing interpose notes already outline the injection & pass-through rules we honor here.

---

### References you may want handy

- Apple/Unix `kqueue(2) / kevent(2)` (structure, uniqueness rule, call semantics). ([Apple Developer][2])
- `EVFILT_VNODE` flags (what apps expect). ([Manual Pages][1])
- `EVFILT_USER` semantics (24-bit flag space + `NOTE_TRIGGER`). ([Debian Manpages][3])
- Passing FDs with `SCM_RIGHTS` over Unix-domain sockets. ([X-CMD][4])
- Your internal interpose plan (hook `kevent`, inject vnode events, preserve others).

If you want, I can turn this into a drop-in C/Rust scaffold (shim + daemon) that compiles on macOS and demonstrates end-to-end flow with a tiny in-memory FsCore that emits writes/renames.

[1]: https://man.bsd.lv/kqueue.2?utm_source=chatgpt.com 'kqueue(2) - Manual pages'
[2]: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kevent.2.html?utm_source=chatgpt.com 'Mac OS X Developer Tools Manual Page For kevent(2)'
[3]: https://manpages.debian.org/bookworm/freebsd-manpages/kqueue.2freebsd.en.html?utm_source=chatgpt.com 'kqueue(2freebsd) — freebsd-manpages — Debian bookworm — Debian Manpages'
[4]: https://man.x-cmd.com/man7/unix?utm_source=chatgpt.com 'unix | x-cmd man (man7) | sockets for local interprocess communication'
[5]: https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/KernelQueues/KernelQueues.html?utm_source=chatgpt.com 'Kernel Queues: An Alternative to File System Events'
[6]: https://manpages.org/unix/7?utm_source=chatgpt.com 'man unix (7): Sockets for local'
