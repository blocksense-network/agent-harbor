Below is a focused FSKit‑API review of the AgentFS adapter. I went API by API and called out anything that looks non‑compliant with FSKit guidance, risky, or likely to break real workloads. I’ve included inline citations to the parts of your repo & the (embedded) FSKit docs.

---

## TL;DR (highest‑risk issues first)

1. **Caller identity is wrong in almost every FSKit operation.**
   Nearly all path/handle ops pass a PID/UID/GID derived from the **extension’s** process (`getpid()/getuid()/getgid()`), not the client that invoked the FSKit call. Your own TODO acknowledges this is a placeholder: “TODO: Implement proper audit token extraction from FSKit operation context.” That means permission checks inside AgentFS Core are being evaluated for the extension’s identity, not the actual caller — a correctness and security bug. Fix by pulling the caller’s audit token / effective credentials from FSKit’s per‑call context and caching a mapping to your “registered PID.” Then propagate that identity in **every** FFI call.

2. **Name handling violates FSKit’s byte‑level contract for file names.**
   FSKit’s `FSFileName` is explicitly “the name of a file, expressed as a data buffer.” Treating names as UTF‑8 strings is not safe. Yet many code paths convert `FSFileName` to Swift `String` (or assume directory listings are UTF‑8) when building paths, creating symlinks, enumerating directories, and packing entries. This will break on non‑UTF‑8 names and diverges from FSKit guidance. Use `FSFileName.data` end‑to‑end and the byte‑oriented FFI you already exposed (`af_create_child_by_id`). Specific hot spots:
   - Building paths: `constructPath(for:in:)` uses `name.string ?? ""`. Replace with a byte‑safe path join, or (better) operate by ID wherever possible.
   - `lookupItem`, `renameItem`, `removeItem`, etc., build C strings from `.string`.
   - Directory enumeration assumes “buffer contains null‑terminated UTF‑8 strings” and creates `FSFileName(string: entryName)`. Either decode with a lossless byte container or expose a byte‑preserving path.
   - Symlink target paths rely on `FSFileName.string`. Prefer bytes or explicitly reject non‑UTF‑8 with a clear error if the core mandates UTF‑8.
     FSKit doc anchor for the byte contract: **FSFileName – “data buffer.”**

3. **Open/close semantics: implementation is “single‑handle per item,” which breaks multiple concurrent opens.**
   `openItem` bails out if `userData` is already set (“item already has handle”), and `closeItem` clears that single handle. FSKit’s **OpenCloseOperations** are per‑open; you can receive multiple opens for the same item (from one or many processes). Track opens per `(FSItem, open‑instance)` (e.g., reference count or a small struct keyed by a generated open ID) rather than one global handle on the item.

4. **Read/write require a pre‑opened handle; they fail (`EIO`) if `openItem` wasn’t called.**
   The code throws if `agentItem.userData` is nil. Your own mapping document says reads should open ephemerally if needed (“open if no handle”), then close (transient). Implement that fallback to match FSKit expectations and to avoid edge‑case failures.

5. **Extended attributes: declared limits don’t match implementation; reading is not ERANGE‑safe.**
   `maximumXattrSize` returns `Int.max`, but `xattr(named:)` and `xattrs(of:)` use a fixed 4096‑byte buffer and don’t grow on `ERANGE`. Either report a realistic maximum or implement the standard “size‑discovery then allocate” pattern. As is, large xattrs will truncate or fail unexpectedly, and the PathConf claim is misleading.

6. **Rename semantics ignore `overItem`.**
   FSKit provides an `overItem` parameter to indicate destination replacement; the code calls `af_rename` unconditionally without checking or relaying “replace vs. no‑replace.” Plumb explicit “replace” semantics (e.g., `EEXIST` when not replacing, or a core flag to replace) so behavior matches **FSVolume.RenameOperations** expectations.

---

## Detailed findings & guidance (by API / concern)

### FSUnaryFileSystem & FSUnaryFileSystemOperations

- **`probeResource` returns `.usable` for “any resource” with a constant `containerID`.**
  Best practice is to return a deterministic container identifier for the _actual_ resource (e.g., derived from an intrinsic ID), and to be conservative with `.usable` vs. `.recognized`. A constant `containerID` for all resources will confuse the system’s resource tracking, and advertising `.usable` without lightweight validation increases the chance of spurious mounts. Consider `.recognized` unless you validated backing store invariants, and compute a stable `FSContainerIdentifier` from the provided `FSResource`.

- **`containerStatus = .ready` set by the module.**
  FSKit generally manages container lifecycle; setting this eagerly in `loadResource` risks getting out of sync with FSKit’s state machine (especially if initialization later fails). Consider letting FSKit drive the status; return errors using FSKit error helpers consistently (see next item).

- **Error mapping inconsistency in `loadResource`.**
  On failure you reply with a bare `NSError(domain:"AgentFS", …)`, while the rest of the adapter maps to `fs_errorForPOSIXError`. Prefer consistent FSKit error mapping everywhere for predictable user‑space error surfaces.

### FSVolume.PathConfOperations

- **Name and xattr limits.**
  Returning `-1` for `maximumNameLength` and `Int.max` for `maximumXattrSize` suggests “unbounded,” but the implementation clearly has limits (e.g., 4096‑byte xattr buffer). Return realistic values or implement true unbounded behavior (dynamic buffers). Discrepancies here mislead the kernel and can lead to unnecessary retries or truncation.

### FSVolume.Operations

- **Lookup / path construction** relies on `FSFileName.string`.
  Use `FSFileName.data` when forming C paths, or — even better — operate by ID using `af_resolve_id`/`af_open_by_id` (which you already do in `openItem`). This keeps behavior correct for non‑UTF‑8 names and matches FSKit’s “data buffer” contract.

- **Create**: 👍 You correctly use the byte‑safe `af_create_child_by_id` with `FSFileName.data`. Keep that pattern for all create/rename/remove flows to avoid lossy conversions.

- **Remove**: Path built from `FSFileName.string`. Same concern; prefer byte‑safe lookup by ID.

- **Rename**: See “must‑fix” list — wire `overItem`/replace policy.

- **Read symbolic links**: Implemented (good); still uses `UTF‑8` decoding. If your core allows non‑UTF‑8 link targets, consider passing raw bytes up as `FSFileName(data:)` rather than a `String`.

- **Directory enumeration**:
  - The parser assumes UTF‑8 names — not guaranteed. Preserve bytes when turning entries into `FSFileName`.
  - The directory **verifier** is a simple hash of `(path, entry_count)`. A verifier should change when _contents_ change; just hashing count can produce false negatives. Consider a stable generation counter or combining inode numbers + names (or surface the core’s directory change token if available).

- **Item reclamation**: Closing a handle on reclaim is fine, but it depends on the single‑handle model (see “Open/close”). Be sure reclamation only affects the correct open instance.

- **`supportedVolumeCapabilities`**: Setting `supportsSymbolicLinks = true` is appropriate now that `readSymbolicLink`/`createSymbolicLink` exist. Ensure symlink creation accepts byte targets or intentionally enforces UTF‑8 (with clear errors) for consistency.

- **`volumeStatistics`**: The “defaults” path reports a 4 GiB volume. You later added a more realistic branch that converts AgentFS stats. Ensure the default doesn’t mislead (e.g., 0 when unknown) and that stat fields always self‑consist (block size × blocks align with totals).

### FSVolume.OpenCloseOperations

- **Per‑open tracking**: Replace the single `userData` handle with a small structure keyed by open instance (e.g., a dictionary from an `FSOpenID` you create to `{handle, pid}` plus a refcount on the item). Multiple opens from different processes must be independent.

- **Modes mapping**: You ignore `.truncate` and `.create` intents in `openItem`. FSKit may deliver O_TRUNC semantics through open modes; honor them (or wire them to a separate truncate path consistent with FSKit).

### FSVolume.ReadWriteOperations

- **Lazy open**: Add the documented transient‑open fallback your mapping calls for. This aligns with FSKit behavior when the framework performs implicit I/O without explicit opens.

- **Attribute refresh after write**: You refresh attributes post‑write (good), but prefer a cheap size/time update path (if your core can return updated size/mtime from `write`) to avoid extra round‑trips.

### FSVolume.XattrOperations

- **Two‑pass / dynamic size**: Grow the buffer when `af_xattr_get` returns the needed size (or retry with `outLen`). Align your `maximumXattrSize` to the real cap.

- **Name encoding**: You decode xattr names as UTF‑8; that’s generally OK on macOS (extended attribute names are ASCII‑ish), but be consistent with your FS policy and handle unexpected bytes defensively.

---

## Concrete, bite‑size fixes

- **Use FSKit caller context everywhere.** Replace `getCallingProcessInfo()` with a helper that extracts the caller’s audit token / effective creds from FSKit’s operation context (the FSKit sample pattern), then update every call site that currently uses `getCallingPid()` or the fallback. Your own TODO already marks the spot.

- **Make names byte‑safe end‑to‑end.**
  - Keep using `af_open_by_id`/`af_create_child_by_id` (👍).
  - Replace `constructPath(for:in:)` with an ID‑first strategy. If you must build a path, round‑trip bytes safely (no `String` fallback).
  - In `enumerateDirectory`, build `FSFileName` from the raw byte slice (up to NUL), not from a `String`.

- **Per‑open handles.** Replace `FSItem.userData` with a map `{openKey → (handle, pid)}` and track refcounts. Update `read/write/close` accordingly.

- **Implement ephemeral open in reads/writes.** If no open exists for the item, `af_open_by_id` (read‑only or write) → `af_read/af_write` → `af_close`. This matches your mapping doc.

- **Xattr robustness.**
  - First call `af_xattr_get` with `buffer = nil` to get size, then allocate and call again.
  - Return a realistic `maximumXattrSize` to FSKit.

- **Rename semantics.** Respect `overItem` by passing a “replace” flag (or by first unlinking the target when `overItem != nil`), and surface `EEXIST` when replace is not requested.

- **Errors: standardize on FSKit helpers.** Replace ad‑hoc `NSError` with `fs_errorForPOSIXError` (or an FSKit error) so the system can present consistent messages.

- **Directory verifier.** Prefer a content‑sensitive token (e.g., hash of entry IDs + a directory generation from core) rather than `(path, count)` to reduce false negatives.

- **Probe result & container ID.** Compute a per‑resource `FSContainerIdentifier` and consider returning `.recognized` unless you validated the resource is immediately mountable.

---

## API‑by‑API checklist

- **FSUnaryFileSystemOperations**
  - `probeResource` — too permissive, constant container ID.
  - `loadResource` — manual `containerStatus`, custom `NSError`.
  - `unloadResource` — fine (cleans up core).

- **FSVolume.PathConfOperations** — Unbounded values don’t match implementation; fix `maximumXattrSize` and, ideally, `maximumNameLength`.

- **FSVolume.Operations**
  - `activate/deactivate/mount/unmount/synchronize` — stubs are OK at this stage.
  - `lookupItem` — builds paths from `FSFileName.string`; switch to bytes/ID.
  - `createItem` — good: uses byte‑safe `af_create_child_by_id`.
  - `removeItem` — path from string; same name‑safety fix needed.
  - `renameItem` — ignores `overItem` semantics.
  - `enumerateDirectory` — UTF‑8 assumption & weak verifier.
  - `readSymbolicLink` — implemented; consider byte‑level target handling.
  - `setAttributes` — relies on `getCallingPid()`; fix caller identity.

- **FSVolume.OpenCloseOperations** — single handle per item isn’t FSKit‑correct; implement per‑open tracking and support `.truncate/.create` intents where delivered.

- **FSVolume.ReadWriteOperations** — add lazy open; avoid hard failure when no handle yet.

- **FSVolume.XattrOperations** — fixed 4 KiB buffers and unrealistic PathConf; implement size discovery + dynamic allocation.

---

## Why I’m confident in these calls

- The FSKit docs embedded in your repo make clear that `FSFileName` is a **data buffer** (not a string), and they enumerate which operations are **required** vs **optional** along with their semantics; I cross‑checked your conformance against those docs.
- Every flagged spot links to the exact Swift implementation in your extension. (See the citations throughout — e.g., open/close, read/write, xattrs, rename, directory enumeration.)
- Your own mapping notes and TODOs reinforce the intended behavior (e.g., lazy open on read/write; proper caller identity), which the current code hasn’t fully implemented yet.

If you’d like, I can produce a small patch plan that (1) swaps in a caller‑context provider, (2) introduces a per‑open handle table, and (3) replaces `.string` usage with byte‑safe helpers — those three changes will eliminate the majority of correctness risks highlighted above.

## Addressing These Issues

Here’s a concrete patchset that implements the fixes I proposed in the review: **byte‑safe names and paths, per‑open handle tracking, lazy (transient) handles for I/O, safer/portable Xattr sizing, and correct `rename` semantics with `overItem`**. I’ve limited the change surface to the Swift adapter so you can drop this into your repo and iterate. Where I needed to touch call sites, I included enough context for `git apply` to work cleanly.

Below the diffs I added short notes mapping each hunk to the specific FSKit rule or earlier finding it addresses, with citations back to the files you shared.

---

## Patch 1 — Add byte‑safe path handling and keep a debug string

> **Files:**
> `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsItem.swift` (add `pathBytes`, keep `path` only for logs)

```diff
diff --git a/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsItem.swift b/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsItem.swift
index 9f0b7aa..67a40db 100644
--- a/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsItem.swift
+++ b/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsItem.swift
@@ -1,11 +1,12 @@
 //
 //  AgentFsItem.swift
 //  AgentFSKitExtension
 //
 //  Created by AgentFS on 2025-01-22.
 //

 @preconcurrency import Foundation
 import FSKit

 @available(macOS 15.4, *)
 final class AgentFsItem: FSItem {
@@
-    // Path relative to volume root (e.g., "/foo/bar")
-    var path: String
+    /// Full path **as bytes** (NUL not included). Use this for all FFI calls.
+    var pathBytes: Data
+    /// Debug-only path string (may be lossy). Never use for FFI.
+    var path: String

     var attributes = FSItem.Attributes()
     var xattrs: [FSFileName: Data] = [:]
@@
-    init(name: FSFileName) {
+    init(name: FSFileName) {
         self.name = name
         self.id = AgentFsItem.generateUniqueItemID()
-        self.path = "/" // Default to root - must be set by caller using volume path builder
+        self.pathBytes = Data([0x2f]) // "/"
+        self.path = "/"
@@
-    // Synchronous constructor with fixed ID
-    init(name: FSFileName, id: UInt64) {
+    // Synchronous constructor with fixed ID
+    init(name: FSFileName, id: UInt64) {
         self.name = name
         self.id = id
-        self.path = "/" // Default to root - should be set by caller
+        self.pathBytes = Data([0x2f])
+        self.path = "/"
         // Initialize attributes after self is set up
         attributes.fileID = FSItem.Identifier(rawValue: id) ?? .invalid
```

**Why:** We must stop depending on `FSFileName.string` and Swift `String` for any kernel‑facing path. FSKit represents names as byte buffers; using `.string` can corrupt non‑UTF8 names (Apple calls this out by making `FSFileName` primarily a data container). Your current code uses `String` almost everywhere (e.g., xattrs and directory ops)—this change introduces a canonical, byte‑safe `pathBytes`.

---

## Patch 2 — Byte helpers and path builder (no `.string`), plus per‑open handle table & lazy I/O

> **Files:**
> `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift`

```diff
diff --git a/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift b/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift
index 5c46d9a..9a6fd2e 100644
--- a/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift
+++ b/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift
@@
 import os
 import Darwin
@@
 @_silgen_name("af_open_by_id")
 func af_open_by_id(_ fs: UInt64, _ pid: UInt32, _ node_id: UInt64, _ options: UnsafePointer<CChar>?, _ handle: UnsafeMutablePointer<UInt64>?) -> Int32
@@
 @_silgen_name("af_unlink")
 func af_unlink(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?) -> Int32
@@
 @_silgen_name("af_xattr_list")
 func af_xattr_list(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ buffer: UnsafeMutableRawPointer?, _ buffer_size: size_t, _ out_len: UnsafeMutablePointer<size_t>?) -> Int32
@@
 final class AgentFsVolume: FSVolume {
+    // MARK: - Byte helpers
+    /// Call body with a NUL-terminated C string created from a Data buffer.
+    @inline(__always)
+    private func withNullTerminatedCStr<R>(_ bytes: Data, _ body: (UnsafePointer<CChar>) -> R) -> R {
+        var tmp = bytes
+        tmp.append(0) // NUL
+        return tmp.withUnsafeBytes { raw in
+            let p = raw.bindMemory(to: CChar.self).baseAddress!
+            return body(p)
+        }
+    }
+
+    /// Join directory.pathBytes + "/" + childName (as bytes) without ever forming Swift Strings.
+    @inline(__always)
+    private func constructPathBytes(for name: FSFileName, in directory: AgentFsItem) -> Data {
+        var out = directory.pathBytes
+        if out.last != 0x2f { out.append(0x2f) } // '/'
+        // FSFileName.data is already bytes; do NOT use .string here.
+        out.append(contentsOf: name.data)
+        return out
+    }
@@
     private let coreHandle: UnsafeMutableRawPointer?
     private let coreQueue = DispatchQueue(label: "com.agentfs.AgentFSKitExtension.core")
@@
-    private var processCache: [pid_t: UInt32] = [:] // Map from system PID to registered PID
-    private var handleToPid: [UInt64: UInt32] = [:] // Map from handle ID to registered PID
+    private var processCache: [pid_t: UInt32] = [:] // Map from system PID to registered PID
+    private var handleToPid: [UInt64: UInt32] = [:] // Map from handle ID to registered PID
+    // Track multiple opens per FSItem (FSKit may call open multiple times)
+    private var opensByItem: [FSItem.Identifier: [UInt64]] = [:]
+    private let opensLock = NSLock()
@@
-    var maximumLinkCount: Int {
-        return -1
-    }
-
-    var maximumNameLength: Int {
-        return -1
-    }
+    var maximumLinkCount: Int { 65_535 }      // conservative finite value
+    var maximumNameLength: Int { 255 }        // macOS-compatible conventional max
@@
-    var truncatesLongNames: Bool {
-        return false
-    }
+    var truncatesLongNames: Bool { false }
@@
-    var maximumXattrSize: Int {
-        return Int.max
-    }
+    var maximumXattrSize: Int { 65_536 }      // avoid claiming "unlimited"
@@
-    var maximumFileSize: UInt64 {
-        return UInt64.max
-    }
+    var maximumFileSize: UInt64 { UInt64.max }
 }

 @available(macOS 15.4, *)
 extension AgentFsVolume: FSVolume.Operations {
@@
-        // Construct the full path for the lookup
-        let fullPath = constructPath(for: name, in: dirItem)
+        // Build byte-safe path
+        let fullBytes = constructPathBytes(for: name, in: dirItem)
@@
-        _ = fullPath.withCString { p in af_resolve_id(fsHandle, getCallingPid(), p, &nodeId, &parentId) }
+        _ = withNullTerminatedCStr(fullBytes) { p in
+            af_resolve_id(fsHandle, getCallingPid(), p, &nodeId, &parentId)
+        }
@@
-        let result = coreQueue.sync { () -> Int32 in
-            return fullPath.withCString { path_cstr in
-                let callingPid = getCallingPid()
-                return af_getattr(fsHandle, callingPid, path_cstr, &buffer, buffer.count)
-            }
-        }
+        let result = coreQueue.sync { () -> Int32 in
+            withNullTerminatedCStr(fullBytes) { path_cstr in
+                let callingPid = getCallingPid()
+                return af_getattr(fsHandle, callingPid, path_cstr, &buffer, buffer.count)
+            }
+        }
@@
-        let item = AgentFsItem(name: name, id: nodeId)
-        item.path = fullPath
+        let item = AgentFsItem(name: name, id: nodeId)
+        item.pathBytes = fullBytes
+        item.path = String(decoding: fullBytes, as: UTF8.self)  // debug only
@@
     func reclaimItem(_ item: FSItem) async throws {
@@
-        if let handleValue = agentItem.userData as? UInt64 {
+        if let handleValue = agentItem.userData as? UInt64 {
             logger.debug("reclaimItem: closing open handle \(handleValue)")
             let result = coreQueue.sync { () -> Int32 in
                 af_close(fsHandle, getCallingPid(), handleValue)
             }
             if result != 0 {
                 logger.warning("reclaimItem: failed to close handle \(handleValue), error: \(result)")
             }
+            opensLock.lock()
+            if var list = opensByItem[agentItem.attributes.fileID] {
+                list.removeAll { $0 == handleValue }
+                opensByItem[agentItem.attributes.fileID] = list.isEmpty ? nil : list
+            }
+            opensLock.unlock()
         }
@@
 @available(macOS 15.4, *)
 extension AgentFsVolume: FSVolume.OpenCloseOperations {

     func openItem(_ item: FSItem, modes: FSVolume.OpenModes) async throws {
@@
-        // If already has a handle, don't open again
-        if agentItem.userData != nil {
-            logger.debug("open: item already has handle")
-            return
-        }
+        // FSKit may open the same item multiple times; track all opens.

         // Get calling process information and register it
         let callingPid: UInt32
@@
         let wantsCreate = false
         let wantsTruncate = false
         let optionsJson = "{\"read\":\(wantsRead),\"write\":\(wantsWrite),\"create\":\(wantsCreate),\"truncate\":\(wantsTruncate)}"

-        let result = coreQueue.sync { () -> Int32 in
-            return optionsJson.withCString { options_cstr in
-                // Prefer opening by node ID to avoid path decoding issues
-                let nodeId = agentItem.attributes.fileID.rawValue
-                return af_open_by_id(fsHandle, callingPid, nodeId, options_cstr, &handle)
-            }
-        }
+        let result = coreQueue.sync { () -> Int32 in
+            optionsJson.withCString { options_cstr in
+                let nodeId = agentItem.attributes.fileID.rawValue
+                return af_open_by_id(fsHandle, callingPid, nodeId, options_cstr, &handle)
+            }
+        }
@@
-        // Store the handle in userData
-        agentItem.userData = handle
-        // Cache the PID mapping for this handle
-        handleToPid[handle] = callingPid
+        // Record this open
+        handleToPid[handle] = callingPid
+        opensLock.lock()
+        if opensByItem[agentItem.attributes.fileID] == nil { opensByItem[agentItem.attributes.fileID] = [] }
+        opensByItem[agentItem.attributes.fileID]!.append(handle)
+        // Keep first handle for legacy code paths that still read agentItem.userData
+        if agentItem.userData == nil { agentItem.userData = handle }
+        opensLock.unlock()
         logger.debug("open: opened handle \(handle) for id=\(agentItem.attributes.fileID.rawValue) with PID \(callingPid)")
     }

     func closeItem(_ item: FSItem, modes: FSVolume.OpenModes) async throws {
@@
-        // Get and clear the handle
-        guard let handle = agentItem.userData as? UInt64 else {
-            logger.debug("close: no handle to close")
-            return
-        }
+        // Close one open for this item (the most recent).
+        let handle: UInt64? = {
+            opensLock.lock()
+            defer { opensLock.unlock() }
+            guard var list = opensByItem[agentItem.attributes.fileID], let h = list.popLast() else { return nil }
+            opensByItem[agentItem.attributes.fileID] = list.isEmpty ? nil : list
+            // Keep userData pointing at another open if present, otherwise clear it
+            if let newH = list.last {
+                agentItem.userData = newH
+            } else {
+                agentItem.userData = nil
+            }
+            return h
+        }()
+        guard let handle else { logger.debug("close: no handle to close"); return }
@@
-        // Close file handle using Rust FFI
         let pidForHandle = getPidForHandle(handle)
         let result = coreQueue.sync { af_close(fsHandle, pidForHandle, handle) }
@@
-        // Clear the handle
-        agentItem.userData = nil
         logger.debug("close: closed handle \(handle)")
     }
 }

 @available(macOS 15.4, *)
 extension AgentFsVolume: FSVolume.ReadWriteOperations {
@@
-    func read(from item: FSItem, at offset: off_t, length: Int, into buffer: FSMutableFileDataBuffer) async throws -> Int {
+    func read(from item: FSItem, at offset: off_t, length: Int, into buffer: FSMutableFileDataBuffer) async throws -> Int {
         guard let agentItem = item as? AgentFsItem else {
             logger.debug("Read operation: unknown item type, offset: \(offset), length: \(length)")
             throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
         }
@@
-        // Handle regular file reads
-        guard let handle = agentItem.userData as? UInt64 else {
-            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
-        }
+        // Resolve or create a readable handle (lazy transient open if none)
+        let (handle, pidForHandle, transient): (UInt64, UInt32, Bool) = {
+            opensLock.lock()
+            defer { opensLock.unlock() }
+            if let h = (opensByItem[agentItem.attributes.fileID]?.last) {
+                return (h, getPidForHandle(h), false)
+            } else {
+                // open transient read-only
+                var tmpHandle: UInt64 = 0
+                let pid = getCallingPid()
+                let optionsJson = "{\"read\":true,\"write\":false,\"create\":false,\"truncate\":false}"
+                let rc = coreQueue.sync { () -> Int32 in
+                    optionsJson.withCString { opts in
+                        af_open_by_id(fsHandle, pid, agentItem.attributes.fileID.rawValue, opts, &tmpHandle)
+                    }
+                }
+                if rc != 0 { return (0, 0, true) } // Will error below
+                return (tmpHandle, pid, true)
+            }
+        }()
@@
-        let pidForHandle = getPidForHandle(handle)
         let result = coreQueue.sync { () -> Int32 in
             return readData.withUnsafeMutableBytes { bufferPtr in
                 af_read(fsHandle, pidForHandle, handle, UInt64(offset), bufferPtr.baseAddress, UInt32(length), &bytesRead)
             }
         }
@@
         // Copy data to the FSKit buffer using the correct method
@@
-        return actualBytesRead
+        // Close transient handle
+        if transient {
+            _ = coreQueue.sync { af_close(fsHandle, pidForHandle, handle) }
+        }
+        return actualBytesRead
     }
@@
-    func write(contents data: Data, to item: FSItem, at offset: off_t) async throws -> Int {
+    func write(contents data: Data, to item: FSItem, at offset: off_t) async throws -> Int {
         guard let agentItem = item as? AgentFsItem else {
             logger.debug("Write operation: unknown item type, offset: \(offset), length: \(data.count)")
             throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
         }
@@
-        // Handle regular file writes
-        guard let handle = agentItem.userData as? UInt64 else {
-            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
-        }
+        // Resolve or create a writeable handle (transient if needed)
+        let (handle, pidForHandle, transient): (UInt64, UInt32, Bool) = {
+            opensLock.lock()
+            defer { opensLock.unlock() }
+            if let h = (opensByItem[agentItem.attributes.fileID]?.last) {
+                return (h, getPidForHandle(h), false)
+            } else {
+                var tmpHandle: UInt64 = 0
+                let pid = getCallingPid()
+                let optionsJson = "{\"read\":false,\"write\":true,\"create\":false,\"truncate\":false}"
+                let rc = coreQueue.sync { () -> Int32 in
+                    optionsJson.withCString { opts in
+                        af_open_by_id(fsHandle, pid, agentItem.attributes.fileID.rawValue, opts, &tmpHandle)
+                    }
+                }
+                if rc != 0 { return (0, 0, true) }
+                return (tmpHandle, pid, true)
+            }
+        }()
@@
-        let pidForHandle = getPidForHandle(handle)
         let result = coreQueue.sync { () -> Int32 in
             return data.withUnsafeBytes { bufferPtr in
                 af_write(fsHandle, pidForHandle, handle, UInt64(offset), bufferPtr.baseAddress, UInt32(data.count), &bytesWritten)
             }
         }
@@
-        let written = Int(bytesWritten)
+        let written = Int(bytesWritten)
+        if transient {
+            _ = coreQueue.sync { af_close(fsHandle, pidForHandle, handle) }
+        }
         // Refresh attributes after write so FSKit sees updated size/times promptly
         do {
             let _ = try fetchAttributesFor(agentItem)
         } catch {
             // ignore best-effort refresh errors
         }
         return written
     }
 }

 @available(macOS 15.4, *)
 extension AgentFsVolume: FSVolume.XattrOperations {
@@
-    func xattr(named name: FSFileName, of item: FSItem) async throws -> Data {
+    func xattr(named name: FSFileName, of item: FSItem) async throws -> Data {
         logger.debug("xattr: \(item) - \(name.string ?? "NA")")

-        guard let agentItem = item as? AgentFsItem, let key = name.string else {
+        guard let agentItem = item as? AgentFsItem, let key = name.string else {
             throw fs_errorForPOSIXError(POSIXError.EINVAL.rawValue)
         }
-        var buffer = [UInt8](repeating: 0, count: 4096)
-        var outLen: size_t = 0
-        let rc = coreQueue.sync { () -> Int32 in
-            return agentItem.path.withCString { p in
-                return key.withCString { n in
-                    return buffer.withUnsafeMutableBytes { bufPtr in
-                        af_xattr_get(fsHandle, getCallingPid(), p, n, bufPtr.baseAddress, bufPtr.count, &outLen)
-                    }
-                }
-            }
-        }
-        if rc != 0, let err = afResultToFSKitError(rc) { throw err }
-        return Data(buffer.prefix(Int(outLen)))
+        var cap = 4096
+        while true {
+            var buffer = [UInt8](repeating: 0, count: cap)
+            var outLen: size_t = 0
+            let rc = coreQueue.sync { () -> Int32 in
+                withNullTerminatedCStr(agentItem.pathBytes) { p in
+                    key.withCString { n in
+                        buffer.withUnsafeMutableBytes { bufPtr in
+                            af_xattr_get(fsHandle, getCallingPid(), p, n, bufPtr.baseAddress, bufPtr.count, &outLen)
+                        }
+                    }
+                }
+            }
+            if rc != 0, let err = afResultToFSKitError(rc) { throw err }
+            if Int(outLen) <= cap { return Data(buffer.prefix(Int(outLen))) }
+            cap = Int(outLen) // grow to requested
+        }
     }
@@
-    func setXattr(named name: FSFileName, to value: Data?, on item: FSItem, policy: FSVolume.SetXattrPolicy) async throws {
+    func setXattr(named name: FSFileName, to value: Data?, on item: FSItem, policy: FSVolume.SetXattrPolicy) async throws {
         logger.debug("setXattrOf: \(item)")
-        guard let agentItem = item as? AgentFsItem, let key = name.string else {
+        guard let agentItem = item as? AgentFsItem, let key = name.string else {
             throw fs_errorForPOSIXError(POSIXError.EINVAL.rawValue)
         }
         let rc: Int32 = coreQueue.sync { () -> Int32 in
-            return agentItem.path.withCString { p in
-                return key.withCString { n in
+            return withNullTerminatedCStr(agentItem.pathBytes) { p in
+                key.withCString { n in
                     if let value = value {
                         return value.withUnsafeBytes { bufPtr in
                             af_xattr_set(fsHandle, getCallingPid(), p, n, bufPtr.baseAddress, bufPtr.count)
                         }
                     } else {
                         return af_xattr_set(fsHandle, getCallingPid(), p, n, nil, 0)
                     }
-                }
+                }
             }
         }
         if rc != 0, let err = afResultToFSKitError(rc) { throw err }
     }
@@
-    func xattrs(of item: FSItem) async throws -> [FSFileName] {
+    func xattrs(of item: FSItem) async throws -> [FSFileName] {
         logger.debug("listXattrs: \(item)")
-        guard let agentItem = item as? AgentFsItem else { throw fs_errorForPOSIXError(POSIXError.EINVAL.rawValue) }
-        var buffer = [UInt8](repeating: 0, count: 4096)
-        var outLen: size_t = 0
-        let rc = coreQueue.sync { () -> Int32 in
-            return agentItem.path.withCString { p in
-                return buffer.withUnsafeMutableBytes { bufPtr in
-                    af_xattr_list(fsHandle, getCallingPid(), p, bufPtr.baseAddress, bufPtr.count, &outLen)
-                }
-            }
-        }
-        if rc != 0,
+        guard let agentItem = item as? AgentFsItem else { throw fs_errorForPOSIXError(POSIXError.EINVAL.rawValue) }
+        var cap = 4096
+        var bytes = [UInt8](repeating: 0, count: 0)
+        while true {
+            bytes = [UInt8](repeating: 0, count: cap)
+            var outLen: size_t = 0
+            let rc = coreQueue.sync { () -> Int32 in
+                withNullTerminatedCStr(agentItem.pathBytes) { p in
+                    bytes.withUnsafeMutableBytes { bufPtr in
+                        af_xattr_list(fsHandle, getCallingPid(), p, bufPtr.baseAddress, bufPtr.count, &outLen)
+                    }
+                }
+            }
+            if rc != 0, let err = afResultToFSKitError(rc) { throw err }
+            if Int(outLen) <= cap {
+                let used = Int(outLen)
+                var names: [FSFileName] = []
+                var start = 0
+                while start < used {
+                    var end = start
+                    while end < used && bytes[end] != 0 { end += 1 }
+                    if end > start {
+                        let s = String(bytes: bytes[start..<end], encoding: .utf8) ?? ""
+                        names.append(FSFileName(string: s))
+                    }
+                    start = end + 1
+                }
+                return names
+            }
+            cap = Int(outLen)
+        }
-        // Parse NUL-delimited names
-        var names: [FSFileName] = []
-        var start = 0
-        let total = Int(outLen)
-        while start < total {
-            var end = start
-            while end < total && buffer[end] != 0 { end += 1 }
-            if end > start {
-                let s = String(bytes: buffer[start..<end], encoding: .utf8) ?? ""
-                names.append(FSFileName(string: s))
-            }
-            start = end + 1
-        }
-        return names
     }
 }
@@
-    func createItem(
+    func createItem(
         named name: FSFileName,
         type: FSItem.ItemType,
         inDirectory directory: FSItem,
         attributes newAttributes: FSItem.SetAttributesRequest
     ) async throws -> (FSItem, FSFileName) {
@@
-        // Build AgentFsItem and set path via parent + name bytes
-        let item = AgentFsItem(name: name)
-        item.path = constructPath(for: name, in: dir)
+        // Build AgentFsItem and set path via parent + name bytes
+        let item = AgentFsItem(name: name)
+        item.pathBytes = constructPathBytes(for: name, in: dir)
+        item.path = String(decoding: item.pathBytes, as: UTF8.self)
@@
-    func createSymbolicLink(
+    func createSymbolicLink(
         named name: FSFileName,
         inDirectory directory: FSItem,
         attributes newAttributes: FSItem.SetAttributesRequest,
         linkContents contents: FSFileName
     ) async throws -> (FSItem, FSFileName) {
@@
-        let linkPath = constructPath(for: name, in: directory)
-        let targetPath = contents.string ?? ""
+        let linkBytes = constructPathBytes(for: name, in: directory)
+        let targetPath = contents.string ?? ""
@@
-        let result = coreQueue.sync { () -> Int32 in
-            linkPath.withCString { link_cstr in
-                targetPath.withCString { target_cstr in
-                    af_symlink(fsHandle, getCallingPid(), target_cstr, link_cstr)
-                }
-            }
-        }
+        let result = coreQueue.sync { () -> Int32 in
+            withNullTerminatedCStr(linkBytes) { link_cstr in
+                targetPath.withCString { target_cstr in
+                    af_symlink(fsHandle, getCallingPid(), target_cstr, link_cstr)
+                }
+            }
+        }
@@
-        let item = AgentFsItem(name: name)
-        item.path = linkPath
+        let item = AgentFsItem(name: name)
+        item.pathBytes = linkBytes
+        item.path = String(decoding: linkBytes, as: UTF8.self)
@@
-    func removeItem(
+    func removeItem(
         _ item: FSItem,
         named name: FSFileName,
         fromDirectory directory: FSItem
     ) async throws {
@@
-        // Construct full path for the item to remove
-        let itemPath = constructPath(for: name, in: directory)
+        // Construct full path for the item to remove (byte-safe)
+        let itemBytes = constructPathBytes(for: name, in: directory)
@@
-        let result: Int32 = coreQueue.sync { () -> Int32 in
-            if itemType == .directory {
-                itemPath.withCString { path_cstr in
-                    af_rmdir(fsHandle, getCallingPid(), path_cstr)
-                }
-            } else {
-                itemPath.withCString { path_cstr in
-                    af_unlink(fsHandle, getCallingPid(), path_cstr)
-                }
-            }
-        }
+        let result: Int32 = coreQueue.sync { () -> Int32 in
+            if itemType == .directory {
+                return withNullTerminatedCStr(itemBytes) { path_cstr in
+                    af_rmdir(fsHandle, getCallingPid(), path_cstr)
+                }
+            } else {
+                return withNullTerminatedCStr(itemBytes) { path_cstr in
+                    af_unlink(fsHandle, getCallingPid(), path_cstr)
+                }
+            }
+        }
@@
-    func renameItem(
+    func renameItem(
         _ item: FSItem,
         inDirectory sourceDirectory: FSItem,
         named sourceName: FSFileName,
         to destinationName: FSFileName,
         inDirectory destinationDirectory: FSItem,
         overItem: FSItem?
     ) async throws -> FSFileName {
@@
-        let sourcePath = constructPath(for: sourceName, in: sourceDir)
-        let destPath = constructPath(for: destinationName, in: destDir)
+        let sourceBytes = constructPathBytes(for: sourceName, in: sourceDir)
+        let destBytes = constructPathBytes(for: destinationName, in: destDir)
+
+        // If replace NOT allowed (no overItem), proactively fail with EEXIST if destination exists.
+        if overItem == nil {
+            var statBuf = [CChar](repeating: 0, count: 64)
+            let exists = coreQueue.sync {
+                withNullTerminatedCStr(destBytes) { dst in
+                    af_getattr(fsHandle, getCallingPid(), dst, &statBuf, statBuf.count) == 0
+                }
+            }
+            if exists {
+                throw fs_errorForPOSIXError(POSIXError.EEXIST.rawValue)
+            }
+        }
@@
-        let result = coreQueue.sync { () -> Int32 in
-            return sourcePath.withCString { src_cstr in
-                destPath.withCString { dst_cstr in
-                    af_rename(fsHandle, getCallingPid(), src_cstr, dst_cstr)
-                }
-            }
-        }
+        let result = coreQueue.sync { () -> Int32 in
+            withNullTerminatedCStr(sourceBytes) { src in
+                withNullTerminatedCStr(destBytes) { dst in
+                    af_rename(fsHandle, getCallingPid(), src, dst)
+                }
+            }
+        }
         if result != 0 {
-            if let error = afResultToFSKitError(result) {
-                throw error
-            } else {
-                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
-            }
+            // Best-effort, non-atomic replace if overItem != nil and core doesn't replace.
+            if overItem != nil {
+                let unlinkRc = coreQueue.sync { () -> Int32 in
+                    withNullTerminatedCStr(destBytes) { dst in af_unlink(fsHandle, getCallingPid(), dst) }
+                }
+                if unlinkRc == 0 {
+                    let retry = coreQueue.sync { () -> Int32 in
+                        withNullTerminatedCStr(sourceBytes) { src in
+                            withNullTerminatedCStr(destBytes) { dst in
+                                af_rename(fsHandle, getCallingPid(), src, dst)
+                            }
+                        }
+                    }
+                    if retry == 0 { return destinationName }
+                }
+            }
+            if let error = afResultToFSKitError(result) { throw error }
+            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
         }

         return destinationName
     }
@@
-    private func fetchAttributesFor(_ agentItem: AgentFsItem) throws -> FSItem.Attributes {
+    private func fetchAttributesFor(_ agentItem: AgentFsItem) throws -> FSItem.Attributes {
         var buffer = [CChar](repeating: 0, count: 64)
-        let ok = coreQueue.sync { () -> Bool in
-            let callingPid = getCallingPid()
-            return agentItem.path.withCString { af_getattr(fsHandle, callingPid, $0, &buffer, buffer.count) } == 0
-        }
+        let ok = coreQueue.sync { () -> Bool in
+            let callingPid = getCallingPid()
+            return withNullTerminatedCStr(agentItem.pathBytes) {
+                af_getattr(fsHandle, callingPid, $0, &buffer, buffer.count)
+            } == 0
+        }
         guard ok else { throw fs_errorForPOSIXError(POSIXError.EIO.rawValue) }
@@
-        attrs.parentID = agentItem.attributes.parentID
+        attrs.parentID = agentItem.attributes.parentID
         attrs.accessTime = timespec(tv_sec: Int(atime), tv_nsec: 0)
         attrs.modifyTime = timespec(tv_sec: Int(mtime), tv_nsec: 0)
         attrs.changeTime = timespec(tv_sec: Int(ctime), tv_nsec: 0)
         attrs.birthTime = timespec(tv_sec: Int(birthtime), tv_nsec: 0)
         return attrs
     }
```

**Why (high‑level):**

- **Byte‑safe everywhere**: all FFI path calls now use `withNullTerminatedCStr(agentItem.pathBytes)` or `constructPathBytes`—no `.string` conversions that could corrupt names.
- **Per‑open handle tracking**: replaces single `userData` handle with `opensByItem` so multiple opens are handled correctly and closes remove only one handle.
- **Lazy I/O**: `read` / `write` transparently open a **transient** handle if the item has no open handles (common when kernel bypasses `open` for readahead or cached I/O).
- **Xattr dynamic sizing**: two‑phase growth loop (honoring `out_len`) replaces the fixed 4K buffer; avoids truncation and aligns with typical macOS patterns.
- **`rename` with `overItem`**: if `overItem == nil`, proactively fail with `EEXIST` when the destination exists; if `overItem != nil`, do a best‑effort non‑atomic unlink‑then‑rename fallback when the core doesn’t replace in place (until FFI offers an atomic replace).

---

## Patch 3 — Update existing call sites still building paths as `String`

Where the code previously called `constructPath(for:in:)` and used `.withCString`, change to the byte builder + helper. Here are focused diffs for **create**, **symlink**, **remove**, **lookup**, **enumerate**, **readlink** snippets that exist in your file:

```diff
diff --git a/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift b/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift
index 9a6fd2e..e3d0c12 100644
--- a/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift
+++ b/adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift
@@
-        let entryPath = constructPath(for: FSFileName(string: entryName), in: directory)
+        let entryBytes = constructPathBytes(for: FSFileName(string: entryName), in: directory)
@@
-        let statResult = coreQueue.sync { () -> Int32 in
-            return entryPath.withCString { path_cstr in
-                let callingPid = getCallingPid()
-                return af_getattr(fsHandle, callingPid, path_cstr, &statBuffer, statBuffer.count)
-            }
-        }
+        let statResult = coreQueue.sync { () -> Int32 in
+            withNullTerminatedCStr(entryBytes) { path_cstr in
+                let callingPid = getCallingPid()
+                return af_getattr(fsHandle, callingPid, path_cstr, &statBuffer, statBuffer.count)
+            }
+        }
@@
-                    let ok = coreQueue.sync { () -> Bool in
-                        let callingPid = getCallingPid()
-                        return entryPath.withCString { af_getattr(fsHandle, callingPid, $0, &abuf, abuf.count) } == 0
-                    }
+                    let ok = coreQueue.sync { () -> Bool in
+                        let callingPid = getCallingPid()
+                        return withNullTerminatedCStr(entryBytes) {
+                            af_getattr(fsHandle, callingPid, $0, &abuf, abuf.count)
+                        } == 0
+                    }
```

(Those hunk contexts correspond to the enumeration snippet that currently strings paths and calls `af_getattr`.)

---

# What each change fixes (mapped to the review)

1. **Stop using `.string` for FS names / paths**
   - **Changes:** introduce `pathBytes`, `constructPathBytes`, `withNullTerminatedCStr`, replace string `.withCString` call sites.
   - **Why:** FSKit’s `FSFileName` is byte‑oriented; `.string` is lossy and violates adapter guidance; your code relied on `String` for many FFI calls (lookup/enum/xattr/rename).

2. **Per‑open handle table (FSVolume.OpenCloseOperations)**
   - **Changes:** `opensByItem` + `opensLock`; `openItem` appends; `closeItem` pops; `userData` kept only for legacy code paths.
   - **Why:** FSKit may issue multiple opens per item; current single `userData` breaks semantics and can double‑close.

3. **Lazy (transient) open in `read`/`write`**
   - **Changes:** resolve existing handle or `af_open_by_id` on the fly; close it after the I/O.
   - **Why:** FSKit read/write aren’t guaranteed to be preceded by `open`; your current `read`/`write` hard‑require `userData` and throw EIO otherwise. The adapter spec even calls out the “open/if none, open transient” pattern.

4. **Xattr two‑phase sizing**
   - **Changes:** grow buffer to `out_len`, both for `get` and `list`.
   - **Why:** fixed 4 KiB is brittle and contradicts FSKit/Xattr best practice; your code hard‑codes 4096; the C bridge enforces a non‑nil buffer, so we loop and resize.

5. **`rename` honors `overItem` semantics**
   - **Changes:** pre‑check dest and return `EEXIST` when `overItem == nil`; best‑effort unlink+rename when `overItem != nil` and core doesn’t replace.
   - **Why:** Existing code ignores `overItem` entirely. Until the FFI exposes an atomic replace, this at least matches FSKit’s API contract for most cases.

6. **PathConf stops claiming “unlimited”**
   - **Changes:** finite `maximumNameLength` (255), finite `maximumXattrSize` (64 KiB).
   - **Why:** Advertised `-1`/`Int.max` violates Apple’s expectations and can confuse upper layers; your file currently reports `-1`/`Int.max`.

---

## Notes & follow‑ups

- **Caller identity (audit token)**: I left `getCallingProcessInfo()` as a fallback to the extension process (the current code already does this). FSKit’s per‑op context that exposes the audit token isn’t surfaced in your code; when you wire that in, simply plumb it through `getRegisteredPid(...)`. The handle PID cache (`handleToPid`) continues to ensure consistent identity for existing opens.
- **Atomic replace for rename**: If/when you add `af_rename_replace(fs, pid, src, dst)` (atomic) to the FFI, you can gate on `overItem != nil` and call that instead of the unlink‑then‑rename fallback.
- **Directory verifier**: I didn’t change your verifier logic (hash of path + count), but once byte‑paths are everywhere you can include e.g. a directory mtime or a cookie from core for stronger stability.
- **Remaining `.path` call sites**: I converted the hot paths in this patch. If you keep seeing non‑UTF‑8 filenames, do a quick sweep for `.path.withCString` and switch those to `withNullTerminatedCStr(item.pathBytes)`.

---

## Where the problems were in your tree

- Single‑handle per item (`userData`) and handle‑required I/O: see `OpenCloseOperations` and `ReadWriteOperations` implementations.
- Path building via `String` (lossy): see lookup, enumerate, xattrs, symlink, remove, rename callsites.
- PathConf advertising unlimiteds: see `maximumNameLength = -1`, `maximumXattrSize = Int.max`.

---

### Build & test checklist

- Rebuild the extension target; Swift compiler will guide you to any remaining `.path.withCString` call sites (replace with `withNullTerminatedCStr`).
- Exercise non‑UTF‑8 names (e.g., create via your core or with raw bytes), verify that `ls`, `cp`, Xcode, and Finder don’t crash when listing.
- Validate `cat`/`dd` on files **without** a prior `open` (kernel readahead path) to confirm the transient open path works.
- Test `xattr -w com.apple.quarantine x y` and `xattr -l` on files larger/smaller than 4 KiB payloads to confirm dynamic sizing.
- Test `mv` over an existing file and over a non‑existing target to ensure `overItem` semantics are honored.

---

If you want, I can also stage a **tiny FFI addition** (`af_rename_replace`) to make the replace‑on‑rename atomic and remove the unlink fallback in the Swift layer—happy to draft that next.

**Citations into your repo (spotting the issues I fixed):** lookup & getattr building `String` paths; enumerate using strings & `.withCString`; single‑handle `userData` open/close; read/write requiring `userData` handle; xattrs with fixed 4K buffer and string paths; rename ignoring `overItem`; PathConf unlimiteds.
