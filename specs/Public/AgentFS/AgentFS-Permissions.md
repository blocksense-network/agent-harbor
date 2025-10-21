## AgentFS Permissions and Ownership (Spec)

This document defines the ownership, permission, and error semantics observed by AgentFS.
AgentFS defaults to POSIX-like behavior. A Windows-compatibility mode is available via configuration.

### Terminology

- Process identity: Each operation is performed by a registered process `PID` with identity `(uid, gid, supplementary_groups[])`.
- Object: A file system node (regular file, directory, symlink) stores: `owner_uid`, `owner_gid`, and `mode` (Unix permission bits).

### Configuration

- `security.enforce_posix_permissions: bool`
  - false: allow-all (no permission checks). true: enforce rules below.
- `security.enable_windows_acl_compat: bool`
  - false: POSIX semantics for open/unlink/rename/share. true: diverge where noted (delete/share behavior).
- `security.root_bypass_permissions: bool` (default: false)
  - false: root is subject to the same permission checks as any user. true: root bypasses discretionary checks (typical Unix behavior); execute still requires at least one x bit when executing a file.

### Permission Bits (POSIX mode)

- The effective access class is chosen in order: owner → group (primary gid or any supplementary group) → other.
- Mode bits are interpreted per class: read (r), write (w), execute (x).
- Mapping from `mode` (octal):
  - user: 0o400 (r), 0o200 (w), 0o100 (x)
  - group: 0o040 (r), 0o020 (w), 0o010 (x)
  - other: 0o004 (r), 0o002 (w), 0o001 (x)

### Access Rules

Assuming `enforce_posix_permissions = true`:

- Regular files
  - Read requires r in the chosen access class
  - Write requires w in the chosen access class
  - Execute (if implemented) requires x in the chosen access class

- Directories
  - Traverse a directory component in a path: requires x on that directory
  - List entries (readdir): requires r and x on that directory
  - Create/Remove/Rename an entry within a directory: requires w and x on that directory

- Symlinks
  - `readlink` does not depend on the symlink's own mode; directory traversal rules still apply to path components
  - Opening symlinks as regular files for read/write is not supported

### Ownership and Mode Changes

- `set_owner(path, uid, gid)`
  - Only `uid == 0` (root) may change the owner uid
  - The owner may change the group gid only to a group they belong to (primary or supplementary); otherwise AccessDenied
  - Changing owner or group updates `ctime`
  - Changing owner or group clears setuid/setgid bits on regular files

- `set_mode(path, mode)`
  - Only the owner or root can change `mode`
  - Permission bits (0o777) are enforced for access checks; special bits (setuid 0o4000, setgid 0o2000, sticky 0o1000) may be set by owner/root. Current enforcement: sticky-bit semantics on directories (see below). AgentFS does not implement privilege elevation via setuid/setgid execution.
  - Updates `ctime`

### Special Mode Bits

- setuid (0o4000), setgid (0o2000)
  - Stored when set; cleared on ownership change. AgentFS does not change effective credentials on exec; these bits have no execution side-effect at present.
- sticky (0o1000) on directories — restricted deletion
  - When a directory has sticky bit set, only the file's owner, the directory's owner, or root may unlink/rename entries within it. Otherwise, w+x on the directory suffices.

### Unlink, Rename, and Open Semantics

- Unlink (remove name)
  - Requires w and x on the parent directory
  - The file's own permissions do not control unlink
  - POSIX behavior: after unlink, existing open handles remain valid until last close (delete-on-close)

- Rename
  - Within a directory: requires w and x on that directory
  - Across directories: requires w and x on both source and destination directories; replacing an existing destination entry requires permission to delete within destination directory

- Windows compatibility differences (`enable_windows_acl_compat = true`)
  - Deletion of open files may be blocked by share/deny semantics (delete restricted unless explicitly shared)
  - Where share flags are in conflict, operations fail with AccessDenied

### ACL Extensions (Optional)

- Model
  - Objects may have an ordered list of Access Control Entries (ACEs): `(principal, allow|deny, rights, inheritance_flags)`.
  - Principals cover users/groups (UID/GID) and well-known identities (e.g., owner@, group@, everyone@).
  - Rights include: read, write, execute/traverse, delete, change_acl (WRITE_DAC), change_owner (WRITE_OWNER), read_attrs.

- Evaluation
  - Deny/allow are evaluated in ACE order; the first matching deny for a requested right blocks access; otherwise, a matching allow grants it (Windows/NFSv4 semantics).
  - For POSIX ACLs, only allow entries are used; a mask (if present) limits effective rights of group-class entries.

- Interaction with mode bits
  - If no ACL is present, owner/group/other mode bits define access.
  - If ACLs are enabled, AgentFS may operate in dual-mode (recommended): owner@/group@/everyone@ ACEs stay in sync with mode bits; chmod updates those ACEs; setting ACLs updates the summarized mode view. Implementations may alternatively treat ACLs as authoritative when present.

- Inheritance
  - ACEs can be marked to inherit to new children (files and/or directories) or be inherit-only. POSIX default ACLs map to inheritable ACEs on directories.

- Administration
  - Changing ACLs requires ownership or appropriate `change_acl` right; changing ownership follows `set_owner` rules (POSIX) or requires `change_owner` right in Windows-compat contexts.

### Error Semantics

- AccessDenied
  - Missing required permission bits on the target object
  - Missing execute (search) permission on any directory component used to traverse the path (prefer AccessDenied over NotFound to avoid information leaks)

- NotFound
  - Target path component does not exist

- NotADirectory / IsADirectory / AlreadyExists
  - Returned for type mismatches or name collisions as appropriate

### Notes

- Root bypass of permission checks is disabled by default; enable `security.root_bypass_permissions` to emulate typical Unix superuser behavior.
- When `enforce_posix_permissions = false`, all access checks succeed (errors are limited to structural issues like NotFound, NotADirectory, AlreadyExists).
