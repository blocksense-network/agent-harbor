# Windows Filesystem Hooks for Interpose Mode

This document specifies the filesystem API hooks required for AgentFS Interpose/FD-forwarding mode on Windows. These hooks enable zero-overhead data I/O while maintaining overlay and branch semantics.

## Strategy

Hook **Win32 wide-char** entry points in `kernel32/KernelBase` and **NT Native** calls in `ntdll` to catch libraries that bypass Win32. Add **CRT hooks** as a belt-and-suspenders for statically linked code. Mediate *path* and *metadata* ops; allow heavy I/O to proceed on the forwarded NTFS handle. Where Windows collapsed operations into `SetFileInformationByHandle`, hook that single choke point.

## A. Open / Create / Directory-handle (**Must-hook**)

* `CreateFileW`, `CreateFile2` (and optionally `CreateFileA`)
* **NT:** `NtCreateFile`, `NtOpenFile`

## B. Directory Enumeration & Attributes (**Must-hook**)

* `FindFirstFileExW`, `FindFirstFileW`, `FindNextFileW`, `FindClose`
* `GetFileAttributesExW`, `GetFileAttributesW`, `SetFileAttributesW`
* **NT:** `NtQueryDirectoryFile`, `NtQueryAttributesFile`, `NtQueryFullAttributesFile`

> Needed to present the **overlay view (upper + lower − whiteouts)** for listings and attribute probes.

## C. Rename / Link / Delete / Mkdir / Rmdir (**Must-hook**)

* `MoveFileExW`, `MoveFileW`, `MoveFileWithProgressW`, `ReplaceFileW`
* `CreateHardLinkW`, `CreateSymbolicLinkW`
* `DeleteFileW`, `RemoveDirectoryW`, `CreateDirectoryW`
* **FD-based mutations:** `SetFileInformationByHandle` for:

  * `FileRenameInfo` / `FileRenameInfoEx`
  * `FileLinkInfo` / `FileLinkInfoEx`
  * `FileDispositionInfo` / `FileDispositionInfoEx` (delete / delete-on-close)
  * `FileEndOfFileInfo`, `FileAllocationInfo`, `FileValidDataLengthInfo`
* **NT:** `NtSetInformationFile` (covers the same info classes)

> This is the core of enforcing **copy-up, whiteouts, and branch semantics** even when the app already holds a real NTFS handle.

## D. Metadata & Times (**Must-hook** path + fd variants)

* `GetFileInformationByHandle`, `GetFileInformationByHandleEx`
* `SetFileInformationByHandle` for `FileBasicInfo` (timestamps), `FilePositionInfo` (optional)
* `SetFileTime`, `GetFileTime`
* **NT:** `NtQueryInformationFile`, `NtSetInformationFile` (for the same classes)

## E. Security / ACLs (**Recommended** in enterprise/dev tool scenarios)

* `GetFileSecurityW`, `SetFileSecurityW` (kernel32 → advapi32)
* `GetNamedSecurityInfoW`, `SetNamedSecurityInfoW` (advapi32)
* **NT:** `NtQuerySecurityObject`, `NtSetSecurityObject` (rarely called directly by apps)

## F. Change Notifications (Watchers) (**Recommended**)

* `ReadDirectoryChangesW`, `ReadDirectoryChangesExW`
* `FindFirstChangeNotificationW`, `FindNextChangeNotification`, `FindCloseChangeNotification`
* **NT:** `NtNotifyChangeDirectoryFile` / `…Ex`

> If an app registers **by path** (not by handle), translate overlay paths → backstore paths so it still sees change events when I/O bypasses the mount.

## G. Reporting / Path Exposure (**Must-hook** to avoid leaking backstore paths)

* `GetFinalPathNameByHandleW` (map the returned path back into the overlay namespace)
* (Optional) `GetLongPathNameW`, `GetShortPathNameW` (if your overlay policy cares)

## H. File Mapping & I/O (no hooks needed for zero-copy)

* `CreateFileMappingW`, `MapViewOfFile(Ex)`, `UnmapViewOfFile`
* `ReadFile`, `WriteFile`, `ReadFileEx`, `WriteFileEx`, `ReadFileScatter`, `WriteFileGather`

> With a **forwarded NTFS handle**, these go straight to the kernel. You don't need to hook them.

## I. Extended Attributes (EAs) & Streams (**Optional**; niche)

* **NT:** `NtQueryEaFile`, `NtSetEaFile` (rare)
* Alternate Data Streams are covered via the existing open path if you preserve `path:stream` syntax.

## J. CRT Belt-and-suspenders (**Recommended** for static/UCRT callers)

Hook Unicode and narrow forms (wide is primary in modern apps):

* Opens/creation: `_wopen`, `_open`, `_sopen`, `_sopen_s`; `fopen`, `_wfopen`, `freopen`
* Path ops: `_wrename`, `_rename`; `_wunlink`, `_unlink`; `_wmkdir`, `_mkdir`; `_wrmdir`, `_rmdir`
* Stat & attrs: `_wstat`, `_stat`, `_wstat64`, `_stat64`, `_wstat64i32`, `_stat64i32`, `_fstat`, `_fstat64`
* Mode/time: `_wchmod`, `_chmod`; `_wutime64`, `_utime64` (and 32-bit variants)
* Enumeration shortcuts sometimes used by CRT internals call Win32/NT you already hooked, but covering these closes gaps with static links.

## Cross-cutting Guidance

### 1) Priority & Fallback

* **Absolute minimum for correctness:** A-D + G on both platforms.
* For **developer-tool fidelity** (IDEs, Explorer integration, installers), add E + F + Windows ACLs/EAs.

### 2) Reentrancy & Bypass Guards

* Maintain an **allowlist/guard** so hooks **don't re-enter** on backstore paths (and lower-only reads, where you choose to bypass).
* Keep a fast per-thread "in shim" flag to avoid recursion on functions like `CopyFile` that call back into opens/stats.

### 3) Namespace Translation

* Centralize **overlay⇄backstore path mapping** and **handle→overlay metadata shims** (for `fstat`, `F_GETPATH`, `GetFinalPathNameByHandleW`).
* When a path op implies copy-up (e.g., `SetFileAttributesW` on a lower-only file), the hook should request the AgentFS server to **materialize upper** first, then apply the mutation.

### 4) Locking & Sharing

* Don't hook `LockFile(Ex)`—the kernel enforces them on the forwarded handle.
* **Windows share modes & delete-on-close:** ensure your `CreateFileW` hook **propagates requested share flags** to the backstore open and mirrors `FILE_DISPOSITION_INFO(Ex)` in overlay state.

### 5) Events/Watchers

* If the app watches **by handle** (`ReadDirectoryChangesW` with a dir handle), forwarding the open is enough.
* If it watches **by path** (`FindFirstChangeNotificationW`), **translate** to backstore paths at registration.

## Testing Matrix (Sanity)

1. **Zero-copy guarantee:** Large read/write/mmap on forwarded handles → no adapter data callbacks observed.
2. **Overlay semantics preserved:** `MoveFile`, `DeleteFile`, `SetFileAttributes`, `SetFileSecurity` on lower-only files trigger copy-up + metadata seeding, whiteouts applied, directory listings reflect overlay.
3. **No path leaks:** `GetFinalPathNameByHandleW` returns overlay paths.
4. **Watchers:** `FindFirstChangeNotificationW` on overlay path receive events when the app writes via forwarded handles.
5. **ACLs/attributes/security:** Round-trip set/get behaves as if working on the overlay.
6. **Share/locking semantics:** Windows share modes, delete-on-close, file locks behave identically to native.
