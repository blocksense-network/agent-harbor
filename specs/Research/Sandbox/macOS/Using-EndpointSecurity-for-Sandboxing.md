# Dynamic Policy Enforcement with EndpointSecurity and Rust on macOS

## Overview of the EndpointSecurity Framework

EndpointSecurity (ES) is a macOS framework for monitoring and controlling system events (file access, process execution, etc.) from user space. It replaces earlier kernel extensions (KEXTs) with **system extensions**, allowing security or sandboxing software to run in user space with kernel-level event visibility<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=Developers%20of%20security%20agents%20have,deal%20with%20the%20somewhat%20tedious>. ES events come in two flavors: **notification events** (which inform you of an action after it happens) and **authorization events** (which allow your agent to _approve or deny_ the action before it completes)<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=being%20some%20standouts,is%20very%20similar%20to%20Microsoft%27s>. Authorization events are key to dynamic policy enforcement, since they let us intercept operations like file opens or network connections and decide (in real time) whether to allow them.

**Entitlements and privileges:** To use ES, an app must be code-signed with the Apple entitlement com.apple.developer.endpoint-security.client<https://newosxbook.com/articles/eps.html#:~:text=Endpoint%20Security%20,be%20approved%20by%20users>. This entitlement is restricted (you request it via Apple Developer programs) and requires the app to run with elevated privileges (typically as root). If these conditions aren’t met, the ES client will fail to connect (e.g. with a “not entitled” or “not privileged” error)<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=Developers%20of%20security%20agents%20have,deal%20with%20the%20somewhat%20tedious>. Additionally, the user must approve the application or extension to enable ES monitoring. In practice, ES clients are usually implemented as **system extensions** that the user enables in System Preferences (System Settings), providing a trusted, tamper-resistant context for your code. Running as a launch daemon (root process) with the entitlement is possible during development, but not recommended for production due to security/tampering concerns<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=Developers%20of%20security%20agents%20have,for%20research%20use%20cases%20it%27s>.

**Event subscription model:** Your ES client subscribes to specific event types (file writes, process execs, network connects, etc.). The ES subsystem will send your client a message for each subscribed event that occurs. If it’s an auth event, your client can allow or deny it by responding with the appropriate result<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=being%20some%20standouts,is%20very%20similar%20to%20Microsoft%27s>. For example, an auth event for file open lets you prevent the file from being opened. It’s important to note that your process (or extension) will need **Full Disk Access** permission from the user to monitor all file events on the system. Without Full Disk Access, macOS will prevent your ES client from seeing events involving certain protected locations (Documents, Downloads, etc.)<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=%28,is%20create%20the%20ES%20client>. In a system extension scenario, you may need to guide the user to add your app/extension to the Full Disk Access list in Security & Privacy preferences.

## EndpointSecurity System Extension Setup and Lifecycle

To implement ES in Rust, you will typically set up a **System Extension** containing your Rust code (or calling into it). Here’s a high-level overview of the setup and lifecycle:

- **Xcode project setup:** Create a macOS app (this will be the **container app** or host) and add a new **System Extension** target of type _Endpoint Security_. Xcode will configure the extension’s Info.plist with the proper extension point. Ensure the extension target has the **Endpoint Security Client** entitlement (com.apple.developer.endpoint-security.client) enabled, and that you have a provisioning profile/certificate from Apple that allows this entitlement<https://newosxbook.com/articles/eps.html#:~:text=Endpoint%20Security%20,be%20approved%20by%20users>. The extension and app should share the same Team ID and be properly code-signed.

- **Extension activation:** The container app is responsible for loading/unloading the extension. Typically, you use the OSSystemExtensionRequest APIs to install or update the extension. When the app first requests to activate the ES extension, the system will prompt the user (an alert in System Preferences/System Settings) to allow it. The user must explicitly approve your extension since it can monitor system activity. Once approved, the system extension is loaded and persists across reboots (it will auto-start on login, similar to a driver).

- **Running context:** The EndpointSecurity extension runs in a separate process (managed by the system, outside of any sandbox). It runs as root with the ES entitlement, communicating with the ES kernel component (endpointsecurityd and the ES kext under the hood<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=ES%20is%20architected%20as%3A%20user,The%20answer%20is%20two%20fold>). This process will start receiving events as soon as it subscribes to them (more on that below). Because it’s a system extension, it benefits from System Integrity Protection – even a root user cannot easily tamper or unload it arbitrarily<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=System%20scope%20daemon%29,the%20above%20requirements%20for%20entitlement>. Uninstalling the extension requires user action (via the container app’s removal or a dedicated systemextensionsctl command).

- **Communication with app (Supervisor):** Often, the extension will need to communicate with a UI or daemon in user space – for example, to prompt the user for a decision. You can establish an XPC connection between the extension and its container app (or a helper daemon). This might involve configuring a Mach service in the extension’s Info.plist or having the container app listen for XPC from the extension<https://stackoverflow.com/questions/79764813/macos-system-extension-xpc-connection-fails-silently-nsxpclistener-delegate-nev#:~:text=macOS%20System%20Extension%20XPC%20Connection,The%20extension>. The AgentHarbor design uses a “Supervisor” process (integrated with the app/CLI) to handle user prompts and policy storage; the ES extension sends queries to this supervisor for decisions.

- **Lifecycle events:** On upgrade or when stopping the extension, you’ll issue a deactivate request. During development, you can use the Terminal (systemextensionsctl list / uninstall) to manage it. The extension’s code should handle graceful shutdown if needed (though generally ES extensions run continuously). Remember that if the user removes the container app, the system will automatically unload the extension<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=Even%20if%20the%20root%20user,are%20a%20few%20different%20options>.

- **Development tips:** During development, if obtaining the ES entitlement is a bottleneck, you can test by disabling SIP (System Integrity Protection) on a test machine and running your ES client as a regular root daemon<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=1,the%20application%20as%20root>. This is not for production, but it can speed up initial coding. With SIP disabled, an unsigned binary with the entitlement (or with the entitlement added via codesign) running as root can connect to ES for testing purposes<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=1,the%20application%20as%20root>. Otherwise, use an Apple Development Certificate with the entitlement in a provisioning profile and run the system extension via Xcode (in Developer Mode on the Mac, which allows loading unsigned system extensions for testing).

With the extension in place and approved, we can now dive into implementing policy enforcement for each category of events: filesystem, process, and network.

## Filesystem Event Policies (AUTH_OPEN and AUTH_EXEC)

Filesystem events cover file system accesses. The two critical auth events for sandboxing are AUTH_OPEN (opening files or directories) and AUTH_EXEC (executing a file as a new process). These allow us to intercept file reads/writes and program launches, respectively. Let’s break down how to use them.

### Subscribing to Filesystem Events

First, you need to subscribe to the desired events using the ES client API. In C, this is done with es_subscribe(client, event_count, event_type_array). In Rust, we can use the safe bindings provided by the **endpoint-sec** crate (which wraps the C API)<https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=At%20runtime%2C%20users%20should%20call,the%20app%20is%20running%20on><https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=to%20,avoid%20stalling%20for%20the%20user>. For example:

use endpoint_sec::{Client, EventType};

fn main() \-\> Result\<(), Box\<dyn std::error::Error\>\> {  
 // Initialize the ES client with an event handler callback  
 let client \= Client::new(handle_event)?;  
 // Subscribe to file open and exec auth events  
 client.subscribe(&\[EventType::AuthOpen, EventType::AuthExec\])?;  
 // ... subscribe to other events as needed (we'll add more later) ...  
 // Keep the client alive and running (in a system extension, the run loop keeps it alive)  
 std::thread::park();
Ok(())  
}

Here, handle_event is a callback function you define to handle incoming events. The ES subsystem will invoke this callback on a background thread for each message. **Important:** When creating the client, ensure your process has the correct entitlements and permissions (root, etc.) or Client::new will return an error. If successful, the client is now receiving events.

Under the hood, Client::new in Rust calls es_new_client to connect to the ES subsystem<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=es_new_client_result_t%20res%20%3D%20es_new_client%28%26client%2C%20,proc.audit_token>. In our callback, we will inspect the es_message_t (wrapped as Message in Rust) to determine the event type and take action.

### When and How AUTH_OPEN Fires

**AUTH_OPEN** events fire whenever a process attempts to open a file system object (file, directory, symlink, etc.) for reading, writing, executing, or any other purpose that involves open(2) or similar calls. This event is delivered _before_ the OS grants access to the file, giving our extension a chance to decide. For example, if a sandboxed process calls fopen("/Users/Alice/secret.txt", "r"), an AUTH_OPEN event will be generated (with the path /Users/Alice/secret.txt) for our client to approve or deny<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=switch%20%28message,auth_kextload%3A><https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=case%20ES_EVENT_TYPE_NOTIFY_EXEC%3A%20%5Blog%20appendString%3A%5BNSString%20stringWithFormat%3A%40%22,event.open.file.path%5D%5D%3B%20break>. This includes opens for read, write, create, and possibly other operations like openat, but not operations that don’t actually open a file handle (e.g. pure stat() calls trigger a different event AUTH_STAT). Directory listings via open of a directory also count as AUTH_OPEN on the directory.

**AUTH_EXEC** events fire when a process is about to execute a new program via execve() (or posix_spawn, which under the hood calls exec). In other words, right after a process calls exec but before the new binary runs, ES will send an AUTH_EXEC event. This event contains information about the new binary file that’s being executed (path, etc.)<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=switch%20%28message,auth_kextload%3A>. If our client denies it, the execution will be aborted (the calling process gets an error instead of launching the new program). If allowed, the program runs normally. In a sandbox context, AUTH_EXEC is crucial to prevent a sandboxed process from escaping or launching unauthorized tools.

A subtle point: When a process calls exec, it also triggers AUTH_OPEN for the binary being executed (since the binary file needs to be opened and mapped). In practice, you might see an AUTH_OPEN for the binary and then an AUTH_EXEC for the process execution. Typically, you’d enforce policy at the EXEC stage (and allow the open of the binary if the exec is going to be allowed). It’s common to subscribe to both, but focus your decision on AUTH_EXEC to block execution of disallowed binaries.

### Enforcing File Access Policies (Blocking/Allowing Opens and Execs)

When handling these events, the general flow is:

1. **Retrieve event details:** The ES Message gives you the event type and associated data. For AuthOpen, you can get the file path being opened. In Rust, the crate provides a File object with a method to get the path. For AuthExec, you get details of the process execution attempt (the executable file, arguments, etc.). For example, with the crate you might match msg.event() to an enum variant:

- match msg.event() {  
   Event::AuthOpen(open_event) \=\> { /\* ... \*/ }  
   Event::AuthExec(exec_event) \=\> { /\* ... \*/ }  
   \_ \=\> { /\* other events \*/ }  
  }

- Each event struct (e.g. open_event) has methods to get the target file’s path, the process performing the open, flags (read/write), etc.

2. **Policy decision:** Determine if this operation should be allowed under your sandbox policy. This could involve:

3. Checking a static allowlist/denylist (e.g., is the path within the sandbox’s allowed FS areas? Is the binary one of the permitted tools?).

4. Checking a cached policy decision (maybe the user already approved this file or binary earlier).

5. Potentially consulting an external policy server or a more complex rule system.

If the policy says “allowed,” you’ll respond allow. If “denied,” respond deny. If it’s not sure (e.g., not encountered before and not in allowlist), you may need to **ask the user**.

1. **Path resolution considerations:** Especially for file paths, ensure you resolve symlinks or relative paths appropriately. In a sandbox-within-a-sandbox scenario like AgentHarbor, the sandboxed process might see a path like /sandbox*root/path/to/file. If you’re using an overlay filesystem (AgentFS) with a custom root, you may need to translate that path to the real filesystem path or vice versa. The design calls for \_canonicalizing the path within the AgentFS root* for policy checks. This means mapping the file request to a stable identifier – e.g., ensuring ../ and symlinks are resolved, and mapping it to the virtual file’s actual location on the host. Failing to do this could let processes bypass rules via symlinks, etc.

2. **Blocking and user approval:** If the action is not immediately allowed or denied by policy, this is where dynamic enforcement kicks in. The ES framework will **block the calling thread** while your handler runs. You can take advantage of this to consult the user (via the supervisor). For instance, if a compile job tries to open a file outside the allowed project directory, your extension can pause that open operation and send a message to the Supervisor/UI: “Process X is trying to open file Y – allow?”. The user can then choose to allow or deny. During this time, the thread is blocked in the kernel (the process is effectively waiting for the open to return). It’s generally safe to do this for reasonable durations, but be mindful: if the user takes too long, macOS might kill your ES client for not responding. **By default, auth events have a timeout (approximately 5 seconds)** – if you don’t respond, the system may assume your extension hung and could terminate it<https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Client%3A%3Asubscribe%28%29%20,avoid%20stalling%20for%20the%20user>. In practice, you can stretch this a bit (and Big Sur+ relaxed it so the client isn’t killed outright, but the event might be auto-allowed/denied to not stall the system). Still, you should respond in a timely manner or implement a mechanism to fail safe (e.g., default-deny if no answer in X seconds, as a “timeout deny”).

3. **Respond to ES:** Once you have a decision, call the respond function. In C: es_respond_auth_result(client, message, ES_AUTH_RESULT_ALLOW, cache_flag). In Rust, the endpoint-sec crate provides convenient methods. For example, if you want to allow an AuthOpen:

- msg.respond(ESAuthResult::Allow)?;

- (Or there may be helper like msg.allow()? depending on the crate version.) This call tells the ES subsystem your decision. The cache_flag parameter (the last boolean in C API) can be used to indicate that the decision should be cached by the kernel for similar subsequent events. **Warning:** Kernel caching of ES responses is somewhat limited and can be tricky; a safer approach is to implement caching in your own user-space logic (and always pass cache=false to es_respond_auth_result so you get every event)<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=%2F%2F%20For%20now%20just%20auth,res%29%3B%20%7D%20%7D>. AgentHarbor uses its own user-space TTL cache rather than relying on ES kernel caching, for greater flexibility (scoping by project/user etc.).

4. **Result of the decision:** If allowed, the original open or exec call in the sandboxed process will proceed as normal. If denied, the open will return an error (usually EACCESS/EPERM or similar) to the process, or the exec will fail (the process will continue running the old code, or if it was a fork+exec, it might just exit with an error). From the sandboxed app’s perspective, it’s as if the OS refused the operation. For example, denying an open of a file will typically result in the process getting a “Permission Denied” error if it checks errno.

**Policy caching and scope:** To avoid prompting the user repeatedly for the same resource, implement a caching mechanism. The design calls for caches with a TTL (time-to-live) and scoping at multiple levels: user, session, project, org. For instance, if the user allowed reading /usr/include folder, you might cache that decision for the duration of the project (and maybe also allow it for other projects at org level, depending on policy). A simple approach is to keep an in-memory (or on-disk) map of “approved paths” with a timestamp. Before prompting, check the cache: if the path (or a parent path) was recently approved, and it falls under the same scope, skip the prompt and allow automatically. Scoping means you might have separate caches for each project or user context – ensure your extension knows the context (perhaps via the launching parameters of the sandboxed process or an environment variable that identifies the project).

**Example code snippet (Rust, pseudo-code) for handling AuthOpen/AuthExec:**

fn handle_event(client: \&Client, msg: Message) {  
 match msg.event() {  
 Event::AuthOpen(open_event) \=\> {  
 let file \= open_event.target(); // File object  
 let path \= file.path().unwrap_or_default(); // Get path string  
 let pid \= msg.process().pid(); // PID of requesting process  
 log::info\!("AuthOpen request by PID {} for {}", pid, path);  
 if policy_allows_path(\&path) {  
 msg.allow().unwrap();  
 } else {  
 // Not in allow-list; consult user via supervisor  
 if let Some(decision) \= ask_supervisor("open", \&path, pid) {  
 if decision.allow {  
 msg.allow().unwrap();  
 cache_decision(path, decision.scope, true);  
 } else {  
 msg.deny().unwrap();  
 }  
 } else {  
 // No decision (timeout or failure) – default deny for safety  
 msg.deny().unwrap();  
 }  
 }  
 }  
 Event::AuthExec(exec_event) \=\> {  
 let exec_path \= exec_event.target().path().unwrap_or_default();  
 let pid \= msg.process().pid();  
 log::info\!("AuthExec request by PID {} for {}", pid, exec_path);  
 if policy_allows_exec(\&exec_path) {  
 msg.allow().unwrap();  
 } else {  
 if let Some(decision) \= ask_supervisor("exec", \&exec_path, pid) {  
 if decision.allow {  
 msg.allow().unwrap();  
 cache_decision(exec_path, decision.scope, true);  
 } else {  
 msg.deny().unwrap();  
 }  
 } else {  
 msg.deny().unwrap();  
 }  
 }  
 }  
 \_ \=\> {  
 // Handle other events (we'll cover those later)  
 }  
 }  
}

In this pseudo-code: ask_supervisor would be an RPC to the supervisor process which blocks until user responds (or returns None on timeout), and cache_decision stores the allow for future. The decision.scope might indicate whether the user chose “remember for this project” or just “allow once,” etc.

**Auditing:** For every allow/deny, log it. It’s good practice to have an audit trail of what was allowed or blocked, along with context. The extension can log via os*log or send the info to the supervisor to append to a file. The AgentHarbor plan includes an \_append-only audit log with rotation* for all ES decisions.

### Required Entitlements and Permissions for File Events

The file events themselves do not require separate entitlements beyond the base ES client entitlement – once your extension has that, you can subscribe to any ES event type. However, some additional considerations:

- **Full Disk Access:** As noted earlier, to get events for files in sensitive locations (e.g. another user’s files, system data, Mail, Desktop, etc.), your extension (or the containing app) needs Full Disk Access granted by the user<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=%28,is%20create%20the%20ES%20client>. Without it, macOS will silently filter out those events or deny the underlying file access regardless of ES. In an enterprise or development environment, instruct users to grant this to your app (or use an MDM profile if managing machines).

- **System integrity:** If the file in question is very sensitive (e.g. parts of macOS system where even root cannot normally access due to SIP), you might not even get an event or the ability to allow it – but generally, ES covers most file operations except those blocked by SIP entirely. For sandboxed developer tools, this likely isn’t an issue.

- **Process context:** The file access events originate from processes that could be any on the system. If you only care about sandboxed agent processes (and not every process on the machine), you should **filter events** by the process. For example, the ES Message includes the es*process_t of the actor. You might check if message.process.executable-\>path starts with your sandbox runtime path, or if the team ID matches your organization, etc. This way, your policy logic only runs for relevant processes. Otherwise, you’d be prompting the user for every file open on the system (not desirable\!). ES does allow \_muting* events for certain processes or paths<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=,the%20event%20not%20the%20initiating>. For instance, you can mute all events where the process is not your sandboxed one. The code es_mute_process(client, audit_token) can be used to ignore events from certain processes<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=if%20%28ppid%20%21%3D%201%29%20,proc.audit_token%29%3B%20return%3B>. In the example gist, they muted everything except the process they were interested in. In AgentHarbor, since the sandboxed processes might run under certain known conditions, you can set up muting for all other processes.

- **Caching at kernel level:** A note on the es*respond_auth_result(..., cache) flag. If you pass cache=true when allowing, the kernel will auto-allow similar subsequent events without round-tripping to user space \_for that specific process and file* (it’s like a one-time rule in the kernel until the process exits or the file is closed, etc.). Apple designed it for performance (e.g., allow subsequent reads on the same file after you allowed the first open). However, it’s often safer to use cache=false and implement caching in user space, especially if your policy needs to cover patterns (like “all files in this folder”) which the kernel cache won’t understand. The example above uses our own cache_decision instead.

With file (open/exec) control in place, we can enforce a dynamic filesystem sandbox: the agent’s file accesses are checked against policy, and we block (and prompt) on disallowed ones. Next, we’ll tackle process-level controls: signals and debugging.

## Process Event Policies (AUTH_SIGNAL and Debugging Restrictions)

Process events in this context refer to one process affecting another – sending signals or attempting to inspect/attach to other processes. In a sandbox scenario, we want to isolate the sandboxed processes from the rest of the system. Two main concerns are: **signals** (e.g., a rogue sandboxed process shouldn’t kill random system processes) and **debugging/inspection** (a sandboxed process shouldn’t be able to ptrace or debug outside processes, potentially stealing data or altering them).

### Subscribing to Process Authorization Events

EndpointSecurity provides an auth event for signals: ES_EVENT_TYPE_AUTH_SIGNAL (Authorization for sending a signal)<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=%5Blog%20appendString%3A%40,auth_signal%3A>. It also provides events related to debugging. There are a couple of relevant ones:

- **AUTH_SIGNAL:** Fires when a process calls kill() (or equivalent) to send a signal to another process. This covers all signals, not just deadly ones – even harmless signals go through this, but you might, for example, only enforce policy on certain signals or certain target processes. We’ll likely want to _deny signals from inside the sandbox to outside processes_.

- **AUTH_TRACE:** Introduced in macOS 11 (Big Sur), this event fires when one process attempts to trace or attach to another (e.g., using ptrace, or obtaining a task port)<https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Event%20Sudo%20,macos_10_15_1>. This is exactly the event to intercept debugging. If a sandboxed process tries to attach to an outside process (or vice versa), you get an AUTH_TRACE event. (In some documentation this might be called ES_EVENT_TYPE_AUTH_TASK or similar; the EndpointSecurity headers list events like ES_EVENT_TYPE_AUTH_GET_TASK for task-for-pid attempts and an ES_EVENT_TYPE_AUTH_PROC_CHECK for lesser process info queries<https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Event%20Openssh%20Logout%20,macos_14_0_0>. But as of Big Sur/Monterey, AUTH_TRACE is the key one for full attach. We will treat “debugging restrictions” as mostly handled by AUTH_TRACE plus possibly PROC_CHECK.)

- **Other related events:** There is a ES_EVENT_TYPE_NOTIFY_EXEC (process executed) and ES_EVENT_TYPE_NOTIFY_EXIT (process exited) which are notification events. While not directly used to block anything (they are not auth events), you might subscribe to these for bookkeeping (for instance, to know when sandbox processes start or exit, if needed for cleanup or audit). Also ES_EVENT_TYPE_NOTIFY_FORK could be of interest to track process lineage in the sandbox (e.g., if the sandbox spawns children, you know those child PIDs to continue enforcing “inside” rules).

For our purposes, subscribe to AUTH_SIGNAL and AUTH_TRACE (and possibly AUTH_GET_TASK if on a macOS version that uses that for ptrace – the Rust crate likely abstracts this behind Event::Trace). In Rust:

client.subscribe(&\[  
 EventType::AuthSignal,
EventType::AuthTrace, // (or AuthGetTask on older OS)  
 EventType::AuthProcCheck // (optional, for process info queries)  
\])?;

Now our callback will receive events when, say, a sandboxed process sends a signal or tries to attach to another.

### Signal Event Behavior and Policy

When an AUTH_SIGNAL message arrives, it contains information such as the **pid and identity of the source process** (who is sending the signal) and the **target process** (who is intended to receive it), as well as the signal number. The extension can thus implement policies like “process A is not allowed to signal process B.”

In a sandbox context, the policy likely is: A sandboxed process may only send signals to processes _within the same sandbox_ (or perhaps not at all, unless it’s to itself or siblings). In AgentHarbor terms, enforce “inside → inside only” for signals. That means if a sandboxed agent (inside) tries to signal anything outside (e.g., kill \-9 some random system process or even a non-sandboxed user process), it should be blocked. On the other hand, signals between agents of the same sandbox, or the agent signaling itself, could be allowed.

How to implement: we need to distinguish “inside” vs “outside” processes. There are a few ways: \- If all sandboxed processes run under a specific user or group, or have a certain process name pattern, we can identify them by that. \- More robust: perhaps tag them via an environment variable or a property at launch. Since we launch the sandboxed process ourselves (via our launcher), we could communicate its “sandbox ID” to the extension. One approach is to have a table in the extension of active sandboxed PIDs (e.g., when we start a sandboxed session, the supervisor can inform the extension of “these PIDs are sandboxed group X”). Alternatively, if the sandboxed processes drop privileges or run as a different UID, that could be used (but usually they might still be your user, just confined by seatbelt and chroot).

For simplicity, let’s say we can determine sandbox membership by checking if the sending process’s PID is in our known sandbox list or if it has a certain parent process (maybe the sandbox launcher). The design suggests a **“cohort”** concept – same-cohort signals allowed. Cohort could be defined as “processes that share the same sandbox root or session”.

Once we know source and target, the logic is: \- If source is inside sandbox and target is outside (not same cohort), **deny** the signal. \- Optionally, also deny signals from outside to inside if you want to prevent outside processes from meddling with the sandbox (though typically outside processes are not restricted by ES, unless they are unentitled; but since our extension sees all, we could also choose to stop, say, a user accidentally killing the sandbox from outside – but that’s likely not needed or wanted, as developers might legitimately stop their processes). \- If both source and target are inside the same sandbox session, **allow** (they might need to signal each other, e.g., a coordinator process sending SIGTERM to a worker thread). \- If source and target are the _same process_ (a process can signal itself), that’s usually fine (allow). \- We might also allow certain benign signals and only block dangerous ones; but a safer default is block all to outside, regardless of signal number. If needed, refine later (e.g., maybe allow SIGCHLD or SIGPIPE out – though those are usually automatic signals not triggered via kill).

Implementing the response: For each AUTH_SIGNAL event, if it doesn’t meet policy, call es_respond_auth_result(...DENY...). Denying a signal means the kernel will drop that signal – the target process won’t receive it, and the kill(2) call in the source process will return an error (likely EPERM if the kernel thinks it’s not allowed). Our extension could log an audit entry like “Blocked process 1234 from sending SIGKILL to 5678”.

An example snippet (conceptual):

if let Event::AuthSignal(signal_event) \= msg.event() {  
 let sig \= signal_event.sig(); // signal number (e.g., 9 for SIGKILL)  
 let source \= msg.process(); // source process (es_process_t)  
 let target \= signal_event.target(); // target process (es_process_t)  
 if is_inside_sandbox(source) {  
 if \!is_inside_sandbox(target) {  
 // inside \-\> outside: block  
 log::warn\!("Blocking signal {} from {} \-\> {}", sig, source.pid(), target.pid());  
 msg.deny().unwrap();  
 } else {  
 // same sandbox (inside-\>inside): allow  
 msg.allow().unwrap();  
 }  
 } else {  
 // Source is outside (e.g., user pressed Ctrl-C on sandbox process from Terminal)  
 // Usually allow outside-\>inside signals (to let user control their process),  
 // but you could enforce restrictions both ways if desired. Here we allow.  
 msg.allow().unwrap();  
 }  
}

This ensures a sandboxed process cannot kill or affect other processes on the system. It’s a defense-in-depth beyond what macOS Seatbelt might already do (Seatbelt can prevent sending signals to processes with different UID, but if running as same user, seatbelt might not stop it – ES gives us that control).

### Debugging (ptrace) Restrictions

For debugging/inspection: macOS has built-in restrictions (System Integrity Protection and the seatbelt profile can prevent task-for-pid for protected processes). But in our sandbox model, we want to make sure a sandboxed process cannot _debug or inspect outside processes_, and conversely perhaps allow debugging _within_ the sandbox if we intentionally launch a debugger inside.

**AUTH_TRACE** (or AUTH_GET_TASK) events occur when a process requests the task port of another process (which is needed for attaching a debugger)<https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Event%20Sudo%20,macos_10_15_1>. For instance, running lldb \-p \<pid\> will trigger this. Our policy from the design: Allow LLDB/ptrace only within the sandbox cohort; deny attach to outside processes. This aligns with the signal policy: a sandboxed debugger can attach to a sandboxed target (maybe you want developers to debug their code inside the sandbox), but not attach to, say, Safari or any process outside.

So, when an AuthTrace event comes in: \- Identify source and target processes (similar to signals). \- If source is inside sandbox and target is inside same sandbox, allow (so debugging your own sandboxed processes works, perhaps when you enable a “debug mode”). \- If source is inside and target is outside (or vice versa), deny the attach. This prevents a malicious or compromised sandbox process from snooping on other processes. \- If both are outside, it might not involve us – but since our extension sees system-wide, you might choose to not interfere with outside-outside attaches (or possibly you could enforce some org-level policy like disallow debugging certain sensitive processes, though that’s beyond our sandbox scope).

**Note:** There is also ES_EVENT_TYPE_AUTH_PROC_CHECK (introduced in 10.15.4) which is an auth event for lesser inquiries (like obtaining limited info about another process)<https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Event%20Openssh%20Logout%20,macos_14_0_0>. The sandbox seatbelt likely already prevents a lot of that, but you could subscribe and deny those too if needed (to be thorough: for example, blocking calls to kill(pid, 0\) which checks existence of a process, if outside).

Implement handling similar to signals:

if let Event::AuthTrace(trace_event) \= msg.event() {  
 let source \= msg.process();  
 let target \= trace_event.target();  
 if is_inside_sandbox(source) && \!is_inside_sandbox(target) {  
 log::warn\!("Blocking debug attach from {} \-\> {}", source.pid(), target.pid());  
 msg.deny().unwrap();  
 } else {  
 msg.allow().unwrap();  
 }  
}

This ensures no cross-boundary debugging. We would log any denied attempt as well for audit.

Now, **where does this policy get enforced?** Possibly at two levels: We have our ES extension doing it, but also the seatbelt profile applied to the sandbox likely includes rules like deny process-info\* (target outside). Indeed, in the SBPL snippet in the design, they mentioned _process-info hardening_ and signal restriction in the seatbelt profile. That means the sandbox might already block these actions. However, the ES extension still sees the _attempt_ (it might see it even if seatbelt would deny it, or in some cases seatbelt might deny before ES sees it; the exact order is not publicly documented). Running both isn’t redundant: the extension gives you a chance to allow some things in a controlled way that seatbelt denied by default (for example, if you had a debug mode where you _temporarily_ allow in-sandbox debugging, you could override the seatbelt’s broad deny by adjusting the profile or by having that profile allow internal attaches and relying on ES to enforce the boundaries more dynamically).

In summary, subscribe to AUTH_SIGNAL and AUTH_TRACE to enforce inter-process isolation. This yields a sandbox where processes can’t kill or spy on outside world processes, which is important for containment.

### Required Context and Entitlements for Process Events

Again, no extra entitlements beyond the base ES client entitlement are needed for these events. Just ensure you’re running on a new enough macOS for the events you subscribe to (e.g., AUTH_TRACE requires Big Sur+; if you run on Catalina, the AuthTrace subscription might not exist, but AuthGetTask and AuthProcCheck would be the alternatives).

**Permissions and expected process context:** \- The ES extension receives these events system-wide. You must filter to sandbox-related ones. For signals, you might even let through some system signals (for instance, you might not want to interfere if an admin uses kill to stop a sandboxed process, which is outside-\>inside). So decide on filtering policy that matches your security goals without breaking normal control of processes. In AgentHarbor’s case, they specifically mention blocking _inside-\>outside_ and allowing _inside-\>inside_, which implies outside-\>inside (like user stopping a runaway sandbox process) is allowed by omission. \- The **execution order** here: A sandboxed process sends a signal; the kernel pauses _that_ process right at the kill() syscall and sends our extension an AUTH_SIGNAL. We reply, then the kernel either delivers the signal or not. Similarly for trace: the attach call (ptrace) will be paused, we decide, then the kernel allows it or not. The target process doesn’t really know about this unless the action goes through (in which case the target might suddenly have a debugger attached, etc.). So from the target’s view, if we deny, nothing happens. From the source’s view, a denied action usually results in an error code as if it wasn’t permitted.

- **Debugging entitlements:** On macOS, normally to call ptrace() on another process, the source process must have certain entitlements or be root, etc. In a developer scenario, the user might run LLDB which has com.apple.security.get-task-allow or the user entered a password. However, within our sandbox, even if a process somehow has those, our ES can still say “no attach”. So ES gives an extra layer beyond the system’s default restrictions. We don’t need any special entitlements in our extension to monitor this (just the ES entitlement). The sandboxed process itself typically wouldn’t have the entitlement to bypass system restrictions, but if it did, we’d catch it anyway.

By handling process events, we add another wall around the sandbox. Now onto network events, which concern outgoing connections.

## Network Event Policies (AUTH_SOCKET_CONNECT)

Modern development often requires network access (downloading packages, API calls, etc.), but a sandbox should restrict rogue or unexpected network usage. We want to enforce that sandboxed agents **only connect to approved network endpoints**, if at all. By default, we may want to block all external network traffic and let the user approve specific connections (like allowing a package manager to download from a specific domain).

### Subscribing to Network Connect Events

The EndpointSecurity framework provides ES_EVENT_TYPE_AUTH_SOCKET_CONNECT for authorizing outgoing socket connections. This event triggers when a process attempts to initiate a connection via sockets (generally, for TCP/IP or UDP sockets) – for example, a connect() call on a TCP socket will generate this event before the connection is made. By subscribing to AUTH_SOCKET_CONNECT, our extension can allow or deny each outbound connection attempt.

In code, subscribe to AuthSocketConnect. In the Rust endpoint-sec crate, this might be an enum like EventType::AuthConnect or AuthSocketConnect (depending on naming). If the crate lacks a high-level variant, one can use the raw binding constant for ES_EVENT_TYPE_AUTH_SOCKET_CONNECT. Assuming it’s exposed, it would be:

client.subscribe(&\[ EventType::AuthSocketConnect \])?;

You might also subscribe to ES_EVENT_TYPE_AUTH_SOCKET_BIND for completeness (controlling binding to local ports – probably not critical for outbound control) and ES_EVENT_TYPE_AUTH_UIPC_CONNECT (for UNIX domain sockets, which typically we might allow since those are local IPC). But our focus is outbound network.

**Important:** macOS also has a separate framework called NetworkExtension (for firewalls, content filters, etc.). Why use ES vs NetworkExtension? In this design, ES is chosen likely because it unifies with the other controls and can block per thread easily. ES’s AUTH_SOCKET_CONNECT is effective for blocking connections but might not provide as much context (e.g., hostname) as a NetworkExtension would for a DNS-based rule. The AgentHarbor plan even suggests a future integration of a DNS proxy for fine-grained domain rules. For now, we’ll use ES to catch the connect by IP.

### Behavior of AUTH_SOCKET_CONNECT

When a process calls connect() (or an equivalent high-level API that uses it), an AUTH_SOCKET_CONNECT event is generated. The event data will include the socket’s address family, the destination IP address and port, and the process info. For IPv4/IPv6, you get the numeric IP and port. There isn’t a direct hostname in the event (because the kernel deals with IPs), so if the process used a hostname, that was resolved via DNS prior to connect and is not directly in the event. The extension could perform a reverse DNS lookup if needed, but that can be slow or unreliable. Alternatively, one could consult the process’s DNS queries via a DNS proxy if set up.

For our sandbox, likely policy: \- **Default deny** all outbound connections, except perhaps those explicitly allowed. \- **Allow loopback (localhost)** by default, since connecting to services on the same machine (127.0.0.1 or ::1) is generally safe and often needed for development (e.g., a local database or dev server). The design explicitly calls out allowing loopback by default. \- **Allow certain domains** based on user policy. For instance, a user might allow their sandbox to access crates.io or a corporate Git server. When a connection to a new host is detected, the extension will block and prompt the user for approval, similar to the file prompt but for network. \- Possibly **filter by port** as well (maybe you allow port 443 but not random ports? That’s up to policy – could be overkill in this context, but worth noting if needed).

### Blocking/Allowing Connections with Policy Caching

When an AUTH_SOCKET_CONNECT is received: 1\. **Identify the destination:** Extract the IP and port from the event. You can convert the IP to a string for logging/prompting. Optionally, map it to a hostname. A strategy: if the sandboxed app attempted to connect to a hostname, it probably did a DNS lookup just before. If you have a way to intercept or log DNS (NetworkExtension DNS proxy), you could map IP to hostname. Without that, you might just show the IP or try a reverse lookup (not always reliable or might be slow). In a prompt, an IP might be fine, or you can do a best-effort to display something like “93.184.216.34 (example.com)” if you can resolve it.

1. **Apply any pre-defined rules:** e.g., if is_loopback(ip) \-\> allow immediately (no prompt). If the organization has a policy file of allowed addresses or domains, check that. Perhaps developers pre-configure common hosts (like package registries) as allowed to reduce prompt fatigue.

2. **Check cache:** If the user has previously allowed connections to this host (or domain) for the scope (session/project), and it’s still within TTL, then allow automatically. For example, if they allowed api.example.com an hour ago, you might allow all connects to that IP for the rest of the session.

3. **If not sure, prompt the user:** Send the details to the Supervisor UI: e.g., “Sandboxed process X is trying to connect to 93.184.216.34:80. Allow?” The UI could attempt to display a more friendly name (maybe it knows this IP corresponds to example.com from earlier, or it just shows the IP). The prompt might offer choices: Allow once, Always allow for this project, Deny. This is analogous to how Little Snitch or other macOS firewalls prompt, but tailored to our dev sandbox use.

4. **User decision:** If allow, we call es_respond_auth_result(...ALLOW...) for that event. If deny, respond DENY. Also, record the decision in the policy store. If “always allow for project,” record in project-level policy (and populate cache). If “once,” maybe just cache for a short term or just for that exact IP and process.

5. **Caching granularity:** You have options:

6. _By IP:_ simplest, but if IPs change (CDN, DNS round-robin) you might prompt again for what the user perceives as the same host.

7. _By hostname:_ but ES doesn’t give hostname. You could integrate with a DNS monitoring to know that e.g. process X looked up example.com and got IP Y, so when you see connect to Y, you assume host example.com. If that’s engineered, you could store decisions per hostname.

8. The design hints at **domain/IP caching**, meaning they likely cache by some representation of the destination. Possibly they do both: cache the exact IP and also the domain if known.

9. **Enforce TTL:** Unlike file accesses, network connections might be frequent. TTL could be used to automatically expire an allow after, say, an hour. They mentioned decisions have TTL – maybe an allow lasts for the session or a user-specified duration. This prevents a once-allowed host from being forever open if not desired.

10. **Respond within time:** Similar to file events, the connect call is blocked while waiting. The user should respond in a timely manner. If not, we should default deny (because letting a connection hang too long might not break anything, but it could; safer to deny if in doubt). If the supervisor crashed or isn’t responding, default deny ensures nothing leaks (with logging that it happened).

**Performance considerations:** Network events could be high volume (imagine a program making dozens of connections). Prompting for each IP is not acceptable UX. That’s why caching and perhaps grouping are important. A good strategy is to treat all connections to a single domain (or IP block) as one entity for user prompts. E.g., “Allow connections to github.com?” covers all the various IPs of GitHub’s CDN for that session. Achieving that might require a bit of DNS integration outside ES, but it significantly reduces prompt noise. The design even mentions possibly integrating a DNS proxy in the future for fine-grained domain policy – that would allow intercepting DNS queries from the sandbox and applying policy there as well.

**Localhost rule:** Usually allow all loopback traffic with no prompt. You can detect loopback by checking the IP (127.0.0.0/8 and ::1). The ES event for connect will indicate the address; for IPv4, any 127.x.x.x, for IPv6, ::1.

**Audit and logging:** Log all connection attempts and what happened (allowed by policy, allowed by user decision, denied, etc.), along with process and destination. This audit is important for later reviewing what external resources the sandbox accessed.

Example pseudo-code for connect events:

if let Event::AuthSocketConnect(connect_event) \= msg.event() {  
 let pid \= msg.process().pid();  
 let proc_name \= msg.process().executable().path().unwrap_or_default();  
 let addr \= connect_event.destination(); // This could give a sockaddr  
 if addr.is_loopback() {  
 // Allow localhost connections by default  
 log::info\!("Allowing loopback connect by {} (PID {}) to {:?}", proc_name, pid, addr);  
 msg.allow().unwrap();  
 } else if policy_allows_addr(\&addr) {  
 // Pre-approved (by policy or cache)  
 log::info\!("Auto-allowing connect by {} to {} per policy", proc_name, addr);  
 msg.allow().unwrap();  
 } else {  
 // Need user decision  
 log::info\!("Network request from {} to {} requires approval", proc_name, addr);  
 if let Some(decision) \= ask_supervisor_network(pid, \&proc_name, \&addr) {  
 if decision.allow {  
 msg.allow().unwrap();  
 cache_network_decision(addr, decision.scope, true);  
 } else {  
 msg.deny().unwrap();  
 }  
 } else {  
 // No decision (e.g., supervisor unresponsive) \-\> default deny  
 msg.deny().unwrap();  
 }  
 }  
}

The destination() might provide IP/port; you’d format that for logging. ask_supervisor_network would present a prompt possibly including a reverse DNS lookup of the IP for user clarity.

### Entitlements and Permissions for Network Events

Just like with file and process events, the ES client entitlement is sufficient. However, note a couple of things:

- **Interaction with macOS firewall/TCC:** Normally, an app connecting out to the internet doesn’t require user permission (except if it’s a background service on some network extensions). So there’s no TCC prompt for “allow network”. But since our extension is filtering network, the user effectively gets a custom prompt via our supervisor. We should ensure the container app (if it’s the one showing prompts) has whatever network privileges it needs (if it’s just showing UI, no special permission needed; if it’s also doing some network calls, those might require outbound network access allowed in its sandbox if it’s sandboxed – but likely the container app isn’t sandboxed because it has to talk to the extension and such).

- **System extension and NetworkExtension co-existence:** If in future a DNS proxy or content filter is used, those require their own entitlements (com.apple.developer.networking.\*). But that’s beyond our current scope. The design just mentions a stub for potential DNS integration, which implies they considered using a DNS proxy to get domain names. If implementing that, the container app or another extension would need the Network Extension entitlement for DNS proxy.

- **Filtering scope:** As with other events, we likely only want to filter _sandboxed processes’_ network connections, not every process on the system. Without filtering, imagine blocking all system network activity by default – that’d be disastrous. So we must identify if the connection event’s process is one of our sandbox agents. The ES Message.process gives the executable path and team ID of the process initiating the connection<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=,by%20XNU%20and%20by%20user>. If your sandboxed processes have a unique signature (e.g., they run from within your AgentFS chroot, or have a certain parent process), use that to decide. Possibly the extension only prompts for events coming from processes launched via the sandbox launcher. Implementing that check might be done by comparing the process’s parent PID or checking an environment variable (the ES event includes the audit token which contains the UID, PID, etc., and you can fetch the parent PID or process name too). In AgentHarbor, since they control how processes are launched in the sandbox (via their ah sandbox CLI which ultimately calls exec in the sandbox), they can mark or track those PIDs.

- **Performance:** If a sandboxed process is very network-heavy (say running a web server that accepts connections, or doing tons of small connections), the ES approach might incur overhead. Each connection attempt goes through the extension. If performance becomes an issue, one could opt for allowing broad ranges or using a more optimized filtering approach (like granting temporary broad allow and using NE for deeper inspection). However, for typical dev tasks (fetching dependencies, connecting to a few APIs), the overhead is manageable. Just ensure not to do extremely slow operations in the decision path (like a synchronous DNS lookup on the main thread of the extension – that could delay the response beyond the allowed timeframe).

In summary, AUTH_SOCKET_CONNECT gives us a way to implement an outbound firewall for sandboxed processes. With default-deny and user-approved allow rules, we achieve dynamic network policy enforcement. Now let’s discuss how these pieces tie together in the broader AgentHarbor sandbox context, especially with the supervisor and policy management.

## Integration with AgentHarbor Sandbox (Supervisor and Policy Management)

Up to now, we’ve discussed each event type in isolation – how to subscribe and handle file opens, execs, signals, and connects. In a real sandbox system like **AgentHarbor**, these need to work in concert, and there are additional components to manage decisions and user interaction. Here’s how it all fits together:

- **Supervisor daemon/UI:** This is the user-facing part of the system that works with the ES extension. In AgentHarbor, the Supervisor is responsible for receiving queries from the ES extension and presenting the prompt (possibly as a macOS notification, a dialog, or a menubar item UI) for the user to allow/deny. It also records the choice. The communication between the ES extension and the Supervisor could be via XPC. Typically, the extension might have an NSXPCConnection to a service in the container app. When an ES event needs user input, the extension sends a message over XPC (with details like event type, path or address, process info). The Supervisor then activates a prompt UI.

- **Prompt UI and options:** The prompt should clearly convey what is happening (e.g., “Your build process is trying to open file /etc/hosts” or “connect to example.com”) and provide options: _Allow_, _Deny_, and possibly a _Remember this decision_ checkbox or scope selection. The design calls for a “menubar or lightweight app” for prompts with **decision, scope, and remember options**. For instance, the user might choose “Allow for this session” or “Always allow for this project” via the prompt. The Supervisor would then send the decision back to the extension, which would respond to the ES event accordingly.

- **Policy store and scopes:** The system likely maintains a set of policy rules that persist beyond the immediate session. For example, if the user said “Always allow” for a particular resource in a project, that should be saved (perhaps in a configuration file or database keyed by project and resource). The next time the sandbox is launched, it should preload these policies so that the extension can allow known items without prompting. The question mentions **merging policy stores (org → project → user → session) with deterministic precedence**. This means there can be organizational defaults (perhaps set by an admin, applying to everyone), project-level settings (shared among team for that project), user-specific preferences, and session-specific overrides (maybe transient allows). When making a decision, the system should check in that order. For example, org policy might say “deny all access to internal.git.server.com for interns” which would override a user’s attempt to allow it. Or org policy might pre-allow certain safe domains so no one gets prompted for those. Merging them with precedence ensures a clear source of truth.

- **Time-to-Live and expiration:** As discussed, decisions should not necessarily live forever unless explicitly set to “always.” TTL-based caching means even an “always for this project” might be subject to re-validation after some time (maybe the wording “always” means no expiration for that project). A “session” scope means the allow is only until the sandbox is closed; next run, it resets. Ensure the extension or supervisor clears or resets caches appropriately (for instance, when a session ends, drop the session-scoped allows).

- **Audit logging:** The system should log every authorization event somewhere persistent. Each log entry might include: timestamp, process (name, pid, perhaps a unique sandbox session ID), event type (file open/exec/signal/connect), the target (file path, target PID, IP address, etc.), and the outcome (allowed/denied, and whether it was automatic by policy or required user input). Audit logs help in troubleshooting (“Why did my build fail? – Oh, it tried to connect to X and was denied”), as well as in security review (ensuring users aren’t approving something suspicious frequently). The log can be an append-only text file that the supervisor maintains, with periodic rotation to avoid unlimited growth.

- **Integration with sandbox launcher:** The sandboxing system has multiple layers – FS isolation (AgentFS overlay \+ chroot), Seatbelt profile, and ES. When the user runs ah sandbox ..., presumably it sets up the AgentFS, applies the Seatbelt profile (which already denies most things by default), then executes the target process. At that point, our ES extension is already active (running system-wide). Possibly we want to notify the extension of the start of a sandbox session (maybe to set up muting for processes not in sandbox, or to load relevant policy scopes). This could be done by the supervisor when it launches a sandbox: e.g., telling the ES extension “Sandbox session 123 started, containing process group IDs {…}, scope \= project XYZ.” The extension can then tag events with that scope or filter events accordingly. How exactly depends on implementation, but it’s worth noting coordination is needed so the extension knows which processes belong to which project (for caching scope decisions properly). The design references an **AgentHarbor CLI** (ah sandbox command) that orchestrates the launch and presumably hooks into the supervisor for this reason.

- **Defense in depth with Seatbelt:** The static seatbelt (App Sandbox) profile applied at launch already denies a broad swath of actions (filesystem outside a certain path, network, process exec, etc.). If seatbelt denies an action outright, ES might not even get a chance to allow it (since seatbelt would cause an immediate failure). To allow dynamic overrides, the seatbelt profile likely has to be a bit lenient in the areas we want ES to mediate. For example, seatbelt could _allow file read_ everywhere (so the process doesn’t just get blocked by sandbox), trusting ES to then block it except for allowed paths. However, that’s risky because if our ES extension wasn’t running, the sandbox would then have free rein. Instead, a safer approach: seatbelt profile can allow reads only in the AgentFS (safe area) and deny others, but perhaps mark them as _exceptional_ or something. Actually, one mechanism: seatbelt can allow an action but raise a **user-space trigger** that ES sees. There is a concept of “ES event raised by user-space library”<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=notification%20and%20authorization%20,is%20create%20the%20ES%20client>. I’m not certain if that’s used for seatbelt, but potentially, the system could emit an ES event when a seatbelt rule is encountered (though it’s more straightforward: seatbelt denies and log, no user-space hook). It might be that seatbelt is configured in a way that basically “sandboxed process can attempt anything, but ES will catch it” – which is dangerous if ES isn’t present. However, since our system extension is always present once the sandbox command is used (and presumably required), it might be acceptable. The design mentions **ES is the primary enforcement hook for dynamic approvals** and _defense-in-depth_. So likely: seatbelt is set to a restrictive profile (deny most things) to be safe, _and_ ES is layered to catch attempts in a user-friendly way. Possibly seatbelt denies are turned into some form of exception that results in an ES event (not sure if that’s possible; more likely seatbelt just denies and the process gets EPERM immediately, which would bypass ES). If that’s the case, maybe they actually configure seatbelt to be less restrictive but rely on ES \+ chroot. The overview said: AgentFS (overlay FS) enforces read/write policy for file system, and seatbelt ensures the process can’t see outside AgentFS due to chroot \+ seatbelt limiting everything outside. Then ES is used for _dynamic allow-list_ on top of AgentFS for reads outside policy. That suggests seatbelt by itself would block outside reads _unless_ AgentFS somehow marks them differently. It’s complex; for our guide, it suffices to say seatbelt provides a baseline and ES \+ supervisor add an interactive layer.

- **User workflow example:** A practical example tying it together: Developer runs a build in sandbox. The build tries to open /usr/bin/git. Seatbelt might normally deny executing /usr/bin/git, but our policy might allow it if user says okay. Perhaps the seatbelt profile allowed exec of certain developer tools or had a placeholder. The ES extension gets AUTH*EXEC for /usr/bin/git. Our extension pauses the exec, the Supervisor prompt says “Your build wants to execute git – allow it to run?” Developer clicks Allow (and “remember for all sessions” maybe). The extension receives that, responds allow. The git process launches successfully inside sandbox (perhaps also confined by sandbox rules). Then git tries to open github.com network connection. Seatbelt likely denies network outright by default – but maybe the seatbelt profile allowed network egress to proceed (since we want ES to catch it). So ES gets AUTH_SOCKET_CONNECT for IP X. Extension asks “Allow connecting to github.com?” Developer allows and checks “remember for this project”. Extension responds allow, connection proceeds, and caches the decision. The build continues, maybe tries to open some file in /Users/Alice/.ssh/id_rsa. Seatbelt might have denied that (since it’s outside allowed FS). If seatbelt blocked it, the process would error out without ES involvement, which is not ideal because we’d want to prompt. So perhaps the seatbelt profile \_does* allow reads outside but relies on ES. Alternatively, maybe .ssh is not accessible even with allow – maybe that’s intentional. Let’s assume another scenario: build tries to read /usr/include/some.h. We intercept AUTH*OPEN for that include file (since it’s outside AgentFS overlay perhaps). Prompt user “Allow reading system headers from /usr/include?” If user allows, we allow and possibly cache the entire /usr/include directory allow for this project (so subsequent headers don’t cause individual prompts). This would align with \_prompt coalescing* – they mentioned directory-granularity approvals to reduce prompt count. Indeed, if we see multiple opens in the same folder, we could prompt once “Allow access to /usr/include/\*” rather than each file.

- **Performance & hardening:** The plan’s final phases talk about performance tests (lots of ES events, ensuring prompt coalescing and caches hit rate are good). Also fault injection (simulate supervisor crash – does extension safely default deny?). These are considerations to ensure the system is robust. For example, they likely test what happens if 500 files open at once (maybe throttle prompts or allow some batch). They also ensure if the Supervisor isn’t running (maybe user didn’t start it, or it crashed), the extension will not hang forever – probably it auto-denies and logs an error, so the sandboxed process gets an error rather than hanging indefinitely.

- **Security and code signing:** As a system extension with ES entitlement, your code is running with high privileges. Follow least privilege wherever possible. The extension should do minimal work (mostly just decision routing) – the heavy logic can often reside in the supervisor which is easier to update and debug. Also, ensure your extension is signed with the correct entitlements and hardened runtime as required by Apple. The plan mentions _least privilege review and runtime hardening settings_ – e.g., disable unnecessary permissions, enable hardened runtime, etc., in your code signing.

By combining all these, the result is a comprehensive sandbox where: \- **Filesystem:** ES intercepts disallowed file opens/execs and asks for approval, caching results. \- **Process isolation:** ES prevents sandboxed processes from killing or spying on others. \- **Network:** ES blocks outgoing connections except those the user approves, with caching by domain/IP. \- **User control:** The developer (or admin) can make on-the-fly decisions, which are remembered according to policy, via the Supervisor’s friendly prompts. \- **Auditability:** Everything is logged for review. \- **Lifecycle:** When the sandboxed process exits, resources are cleaned up. If the sandbox is restarted, policies persist so previously allowed actions don’t prompt again (unless TTL expired).

## Conclusion

Using the EndpointSecurity APIs on macOS allows us to implement a dynamic, interactive sandbox in user space, and Rust is a viable choice for this given bindings to Apple’s C APIs. We organized the system by event types – filesystem, process, and network – each enforced by ES auth events and coordinated with a supervisor process for approvals.

In summary, the ES system extension (running our Rust code) subscribes to critical auth events and **pauses the sandboxed threads at security boundaries**, delegating decisions to policy logic and the user when needed. For filesystem opens and execs, we verify paths against allow-lists and involve the user for out-of-policy accesses. For signals and debugging, we isolate the sandbox so it cannot interfere with outside processes. For network, we block all by default and let users carve out exceptions for specific hosts as necessary. The guide also underscored the importance of proper entitlements, system extension setup, and the need to run the extension with user approval and full disk access privileges for full visibility.

The **EndpointSecurity framework** provides the low-level mechanism (with Apple’s guarantee of delivery and enforcement), while our **Rust implementation** provides safety, performance, and expressiveness to implement caching and complex policies. By following this structured approach – setting up the ES client, handling each event type, and integrating with a user-space supervisor – you can achieve a flexible sandbox that is secure by default but can adapt to user needs in real time. With thorough testing (unit, integration, performance) and careful policy design, this system enables developers to run tools in a confined environment without overly constraining their workflows, delivering both security and usability in macOS sandboxing.

---

<https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=Developers%20of%20security%20agents%20have,deal%20with%20the%20somewhat%20tedious> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=being%20some%20standouts,is%20very%20similar%20to%20Microsoft%27s> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=Developers%20of%20security%20agents%20have,for%20research%20use%20cases%20it%27s> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=%28,is%20create%20the%20ES%20client> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=ES%20is%20architected%20as%3A%20user,The%20answer%20is%20two%20fold> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=System%20scope%20daemon%29,the%20above%20requirements%20for%20entitlement> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=Even%20if%20the%20root%20user,are%20a%20few%20different%20options> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=,the%20event%20not%20the%20initiating> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=,by%20XNU%20and%20by%20user> <https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview#:~:text=notification%20and%20authorization%20,is%20create%20the%20ES%20client> 5\. Endpoint Security Overview · Brandon7CC/mac-monitor Wiki · GitHub

[https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview](https://github.com/Brandon7CC/mac-monitor/wiki/5.-Endpoint-Security-Overview)

<https://newosxbook.com/articles/eps.html#:~:text=Endpoint%20Security%20,be%20approved%20by%20users> Endpoint Security \- NewOSXBook.com

[https://newosxbook.com/articles/eps.html](https://newosxbook.com/articles/eps.html)

<https://stackoverflow.com/questions/79764813/macos-system-extension-xpc-connection-fails-silently-nsxpclistener-delegate-nev#:~:text=macOS%20System%20Extension%20XPC%20Connection,The%20extension> macOS System Extension XPC Connection Fails Silently

[https://stackoverflow.com/questions/79764813/macos-system-extension-xpc-connection-fails-silently-nsxpclistener-delegate-nev](https://stackoverflow.com/questions/79764813/macos-system-extension-xpc-connection-fails-silently-nsxpclistener-delegate-nev)

<https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=1,the%20application%20as%20root> <https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=es_new_client_result_t%20res%20%3D%20es_new_client%28%26client%2C%20,proc.audit_token> <https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=switch%20%28message,auth_kextload%3A> <https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=case%20ES_EVENT_TYPE_NOTIFY_EXEC%3A%20%5Blog%20appendString%3A%5BNSString%20stringWithFormat%3A%40%22,event.open.file.path%5D%5D%3B%20break> <https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=%2F%2F%20For%20now%20just%20auth,res%29%3B%20%7D%20%7D> <https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=if%20%28ppid%20%21%3D%201%29%20,proc.audit_token%29%3B%20return%3B> <https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64#:~:text=%5Blog%20appendString%3A%40,auth_signal%3A> An example of using the libEndpointSecurity.dylib in Catalina · GitHub

[https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64](https://gist.github.com/mcastilho/1774c12bb8b35be5c03f6c2e268eae64)

<https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=At%20runtime%2C%20users%20should%20call,the%20app%20is%20running%20on> <https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=to%20,avoid%20stalling%20for%20the%20user> <https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Client%3A%3Asubscribe%28%29%20,avoid%20stalling%20for%20the%20user> <https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Event%20Sudo%20,macos_10_15_1> <https://docs.rs/endpoint-sec/latest/endpoint_sec/#:~:text=Event%20Openssh%20Logout%20,macos_14_0_0> endpoint_sec \- Rust

[https://docs.rs/endpoint-sec/latest/endpoint_sec/](https://docs.rs/endpoint-sec/latest/endpoint_sec/)

                  Local-Sandboxing-on-macOS.status.md
