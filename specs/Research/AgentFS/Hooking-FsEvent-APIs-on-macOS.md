# Hooking File Monitoring APIs for AgentFS Interpose on macOS

## Overview

On macOS, applications can monitor file system changes using several APIs. For Agent Harbor's **AgentFS** component, specifically when running in **interpose mode** (using DYLD_INSERT_LIBRARIES via Rust’s redhook, without a visible FUSE/FSKit mount point), we must hook **every public API that apps use to watch files**. The goal is to ensure that file monitoring works seamlessly with AgentFS's virtualized, per-process branch views. We need to intercept monitoring requests, potentially translate paths between the application's overlay view and the AgentFS backstore, filter events based on branch state (e.g., whiteouts), and inject custom notifications reflecting changes within the AgentFS overlay, all without disrupting normal event delivery. Crucially, our hooks must preserve the original threading and delivery model of each API so that applications perceive AgentFS events just like real filesystem events.

Below we outline the key file-watching mechanisms on macOS and how to hook them in user space for the AgentFS interpose shim.

## FSEvents API (Directory Hierarchy Monitoring)

**About FSEvents:** The **File System Events (FSEvents)** API lets apps register for notifications when the contents of a directory tree change[^1]. Under the hood, the FSEvents service delivers batched events asynchronously to the app's callback[^2], typically scheduled on a CFRunLoop. This API is common for monitoring whole directories.

**Hooks for AgentFS Interpose (FSEvents):**

- **Hook FSEventStreamCreate(...):** Interpose this constructor to wrap the app’s callback. We store the original FSEventStreamCallback and context, passing a **replacement callback** to the real FSEventStreamCreate.
  - **Path Translation:** When creating the stream, we may need to translate the watched overlay paths provided by the application to their corresponding paths within the AgentFS backstore, so that the underlying FSEvents mechanism monitors the correct physical locations.
  - **Event Filtering/Injection:** Our replacement callback receives events first. It will:
    - Forward genuine events from the backstore to the original callback, potentially translating paths back to the application's overlay view.
    - Filter out events related to files hidden by AgentFS whiteouts in the current process's branch.
    - Inject **custom notification events** reflecting changes made within the AgentFS upper layer (e.g., file creations, modifications, deletions managed purely by AgentFS).
  - **Threading Preservation:** We ensure the original callback is invoked on the same thread/runloop, preserving the FSEvents threading model.
- **Hook FSEventStreamScheduleWithRunLoop and FSEventStreamStart:** Hooking FSEventStreamStart can be useful to trigger an initial AgentFS-specific notification once the stream is active, or to finalize path translations based on the runloop context if needed. Always call the original functions.
- **Hook FSEventStreamStop/Invalidate:** Optionally hook to clean up stored callback state or path mappings associated with the stream. Always call the original functions.

**Behavior:** The FSEvents hook must **not block or alter real events** unless necessary for AgentFS consistency (e.g., filtering whiteouts). Genuine file events are passed through, potentially path-translated. Custom AgentFS events (e.g., reflecting CoW operations or branch-specific changes) are inserted seamlessly into the stream, formatted as valid FSEventStreamEvent entries. The application sees a coherent stream of events reflecting its virtual AgentFS view.

**Note:** FSEvents is a high-level API abstracting /dev/fsevents[^1][^2]. Hooking its public API (FSEventStream\*) is sufficient for AgentFS interpose; no private hooks are needed.

## **BSD kqueue/kevent (File Descriptor Monitoring)**

**About kqueue:** The **kqueue/kevent** interface provides fine-grained notifications for specific file descriptors (FDs), often opened with O_EVTONLY. Apps register an **EVFILT_VNODE** filter to watch for changes (delete, write, rename, attribute change, etc.)[^4][^5]. Events are retrieved synchronously via kevent(). GCD dispatch sources (DISPATCH_SOURCE_TYPE_VNODE) often wrap kqueue.

**Hooks for AgentFS Interpose (kqueue/kevent):**

- **Hook kevent() (and variants kevent64/kevent_qos):** This is the critical hook. Our interposed kevent will:
  - Call the real kevent system call to get pending kernel events.
  - **Filter/Modify Events:** Examine returned EVFILT_VNODE events. Filter out events for files masked by AgentFS whiteouts in the process's branch. Modify event details if needed (e.g., translate associated paths/IDs, though kqueue is FD-based, context might matter).
  - **Inject Custom Events:** Append fabricated struct kevent entries representing file changes managed purely within the AgentFS upper layer for the current branch.
  - **Adjust Return Value:** Modify the number of events returned to the application to account for filtered/injected events.
- **Preserve Unrelated Events:** The hook must **only target EVFILT_VNODE events** and pass all other event types (sockets, timers, signals) through unmodified[^4].
- **Threading Model:** kevent is synchronous. Injecting/filtering within the hook ensures custom events are delivered on the same thread as real events, preserving the expected model.
- **(Optional) Hook open() for O_EVTONLY:** Hooking open can help AgentFS track which FDs correspond to monitored files, potentially associating them with AgentFS nodes for context during kevent interception[^6]. In interpose mode, since open itself is hooked for FD-forwarding, this context can be captured there.

**Grand Central Dispatch sources:** GCD sources (DISPATCH_SOURCE_TYPE_VNODE) rely on kevent internally[^7]. **Hooking kevent is sufficient to intercept and inject events for GCD sources.** Our modifications will flow through GCD, ensuring the source's event handler block is invoked on the correct dispatch queue with events reflecting the AgentFS branch view.

In summary, hooking kevent covers both direct kqueue users and GCD dispatch sources. The hook adds/filters EVFILT_VNODE events[^5] consistent with the AgentFS overlay state for the calling process's branch, without affecting non-filesystem event types.

## **Legacy File Monitoring APIs (Carbon and Cocoa)**

While less common for modern applications targeted by AgentFS, legacy APIs might be encountered:

- **Carbon FNSubscribe:** An old, voluntary notification system[^9]. If needed, hook FNSubscribe to wrap the callback and inject custom AgentFS events, perhaps by simulating an FNNotify call or directly invoking the callback/posting a Carbon event on the main thread, consistent with AgentFS changes.
- **Cocoa NSWorkspace Notifications:** Posts notifications for user-visible operations (e.g., file moved to trash)[^10]. If an app relies on these, the AgentFS shim can **post custom NSWorkspace notifications** (e.g., NSWorkspaceDidPerformFileOperationNotification) via the standard NSNotificationCenter, mimicking real events to reflect AgentFS operations on the main thread.

These are **public APIs** (though deprecated), making user-space hooking feasible if required for legacy application support within AgentFS interpose mode.

## **Preserving Delivery Semantics and Non-Interference**

A critical goal for AgentFS interpose is **transparency**. Hooks must preserve the original API semantics and avoid disrupting unrelated application behavior:

- **Threading and Runloops:** AgentFS hooks honor the original delivery context. FSEvents callbacks are invoked on the designated runloop thread[^3]. kevent results (including injected ones) are returned on the calling thread. GCD handlers run on their specified queue. Carbon/NSWorkspace notifications are delivered on the main thread. This ensures AgentFS events appear native to the application.
- **No Loss of Real Events:** Hooks pass through all genuine filesystem events, unless filtering is needed for AgentFS consistency (e.g., masking whiteouts). Unrelated events (signals, timers, network) are completely unaffected.
- **Plausible Custom Events:** Injected AgentFS events mimic the format expected by the API (valid flags, potentially translated paths). They represent changes within the AgentFS overlay (e.g., file creation in the upper layer, CoW modifications) and appear as part of the normal event stream consistent with the process's branch view.

## **Public vs. Private API Hooks**

For AgentFS interpose mode, **hooking the public APIs (FSEvents, kqueue/kevent, and potentially legacy Carbon/Cocoa) is sufficient**. These cover the documented ways applications monitor file changes.

- **Private FSEvents Internals:** We do **not** need to hook communication with fseventsd or /dev/fsevents[^2]. Intercepting at the FSEventStream\* API level is adequate.
- **Kqueue:** As a system call interface, kevent is the primary entry point. No private alternatives exist for this type of monitoring.
- **Spotlight/Notifyd:** These are not general file monitoring APIs and are not targeted.

Focusing on public APIs aligns with the user-space interposition model and provides comprehensive coverage for applications interacting with AgentFS.

## **Handling All Application Types**

The described hooking strategy supports various application types running under AgentFS interpose:

- **Modern 64-bit apps (Cocoa):** Typically use FSEvents or GCD dispatch sources, covered by FSEventStream\* and kevent hooks.
- **Command-line/POSIX tools:** Often use kevent directly, captured by our hook. Cross-platform libraries (like Qt) using FSEvents on macOS are also covered. Polling-based tools are unaffected as they don't use event APIs.
- **Legacy Carbon apps:** Covered by FNSubscribe hooks if needed.

This ensures that applications, regardless of their file monitoring approach, receive notifications consistent with their AgentFS branch view when running in interpose mode. Custom AgentFS events are delivered indistinguishably from real system events.

**Sources:** Informed by Apple's documentation on FSEvents[^1], kqueue/kevent[^5], dispatch sources[^7], and legacy APIs[^9][^10]. By interposing these public entry points within the AgentFS shim, we can ensure file monitoring reflects the virtualized AgentFS state without kernel modifications or private API usage.

[^1]: macOS File System Events (FSEvents) Store Database | Detection [https://insiderthreatmatrix.org/detections/DT108](https://insiderthreatmatrix.org/detections/DT108)

[^2]: At the kernel level, file system events are delivered through the fseventsd daemon and /dev/fsevents device

[^3]: FSEvents API - Advanced Mac OS X Programming: The Big Nerd Ranch Guide [https://www.oreilly.com/library/view/advanced-mac-os/9780321706560/ch16s14.html](https://www.oreilly.com/library/view/advanced-mac-os/9780321706560/ch16s14.html)

[^4]: The kernel queues API provides event notification facilities [https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/KernelQueues/KernelQueues.html](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/KernelQueues/KernelQueues.html)

[^5]: The third argument, filter, specifies the type of event to monitor

[^6]: When you only want to monitor a file for events and would not be unmounted [https://developer.apple.com/library/archive/documentation/Performance/Conceptual/FileSystem/Articles/TrackingChanges.html](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/FileSystem/Articles/TrackingChanges.html)

[^7]: file is deleted, written to, renamed, or has its metadata changed

[^9]: For more global changes, the File Manager provides FNSubscribe/FNNotify

[^10]: For Cocoa developers, the NSWorkspace class provides notifications
