## AgentFS Control Messages — Delivery and Wire Contract

### Overview

AgentFS exposes a small control plane to manage snapshots and branches on a mounted filesystem volume. The CLI (`ah agent fs ...`) and other tools communicate with the running user‑space filesystem server (adapter) using OS‑specific transports, but the payloads and semantics are shared.

### Operations

- snapshot.create — create a new snapshot (optional name)
- snapshot.list — list existing snapshots
- branch.create — create a writable branch from a snapshot (optional name)
- branch.bind — bind the current (or specified) process to a branch view

All ops carry a `version` field. Current value: `"1"`.

## New: Backstore lifecycle
- `backstore.create_ramdisk { fs: "apfs|zfs|nilfs|btrfs|ext4|refs|ntfs", size_mb: u32, opts?: {...} } -> { mount, supports_native_snapshots }`
- `backstore.attach_hostfs { root: Path } -> { root }`
- `backstore.status -> { kind, root_or_mount, supports_native_snapshots }`

## New: FD-forwarding / Interpose
- `fd.open { path, flags, access, share } -> { ok, handle } | { err: FORWARDING_UNAVAILABLE, reason: "no_reflink|size_threshold|policy" }`
  - On success: Unix returns handle via `SCM_RIGHTS` on the control socket; Windows duplicates via `DuplicateHandle`.
  - On failure: Server declines FD-forwarding; shim falls back to KBP (mounted volume) for that process/file.
- `path.op { op: "rename|unlink|chmod|..." , args... } -> { ok }`

## New: Interpose policy configuration
- `interpose.set { forwarding: "eager_upperize|disabled", max_copy_bytes: u64, require_reflink: bool }`
- `interpose.get -> { forwarding, max_copy_bytes, require_reflink }`

## New: Windows open-redirect policy (experimental)
- `policy.set { windows: { open_redirect: bool } }`
- `policy.get -> ...`

## Snapshot on backstore
- `snapshot.native { label } -> { id }` (if supported)
- `snapshot.copy_active { label } -> { id }`

### Schemas

- Request SSZ Schema: `specs/Public/Schemas/agentfs-control.request.schema.json`
- Response SSZ Schema: `specs/Public/Schemas/agentfs-control.response.schema.json`

Adapters and clients MUST validate messages against these schemas. Messages are encoded using SSZ (Simple Serialize) format for compact, secure binary serialization. The schemas define the logical structure; implementations MUST use SSZ-compatible types that remain semantically equivalent to the schema and include a version field.

### Error Mapping

- Success responses follow the op‑specific response schema.
- Errors return an Error struct with `error: string` and optional `code: integer`:
  - Windows: adapter maps internal status to NTSTATUS and also returns an error SSZ in the output buffer when applicable.
  - FUSE/FSKit: adapter returns `-errno`; any SSZ body is optional, but recommended for diagnostics.

### Message validation and errors
- All messages are versioned; core returns `ERR_UNSUPPORTED` when capability absent.

### Transports by Platform

#### Windows (WinFsp)

- Transport: `DeviceIoControl` on the mounted volume handle, handled by WinFsp `FSP_FILE_SYSTEM_INTERFACE::Control`.
- Requirements (per winfsp.h `Control` docs):
  - IOCTL code uses a DeviceType with bit `0x8000` set and `METHOD_BUFFERED`.
  - Input/Output buffers are small and copied via the FSD.
- Client steps:
  1. `HANDLE h = CreateFileW(L"\\\\.\\X:", GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|FILE_SHARE_WRITE, ...)`.
  2. Build request SSZ, e.g. encoded `{ "version":"1", "op":"snapshot.create", "name":"clean" }`.
  3. `DeviceIoControl(h, IOCTL_AGENTFS_SNAPSHOT_CREATE, inBuf, inLen, outBuf, outLen, &bytes, NULL)`.
- Server behavior:
  - Parse SSZ request, call `FsCore::{snapshot_create|snapshot_list|branch_create_from_snapshot|bind_process_to_branch}`.
  - Write a response SSZ per schema; map errors to NTSTATUS on return.

#### Linux/macOS (FUSE)

- Transport: `ioctl` on a special control file inside the mount, e.g. `<MOUNT>/.agentfs/control`.
- Client steps:
  1. `int fd = open("<MOUNT>/.agentfs/control", O_RDWR)`.
  2. Issue `ioctl(fd, AGENTFS_IOCTL_CMD, &buffer)` where buffer contains SSZ-encoded request.
  3. On success, read SSZ response from the same buffer; `ioctl` returns `0`. Errors return `-errno`.
- Server behavior:
  - Implement libfuse `.ioctl` for the control file only; ignore for other inodes.
  - Decode SSZ requests, validate against schema, call `FsCore` methods, and encode response SSZ to user buffer.

#### macOS (FSKit)

- Preferred transport: XPC to the FS extension. Define a narrow XPC interface that carries SSZ-encoded request/response per the schemas above.
- Fallback: the control file approach identical to FUSE (`<MOUNT>/.agentfs/control`).
- Server behavior:
  - XPC endpoint decodes SSZ request, calls `FsCore`, and returns SSZ-encoded response.

### Security and Access Control

- Restrict control plane access to trusted principals:
  - Windows: inspect caller token in the Control handler.
  - FUSE: set strict permissions on `.agentfs` (e.g. root:root, 0700) or validate UID/GID at the adapter.
  - FSKit: enforce entitlement and XPC validation; deny untrusted senders.
- Validate string sizes and enforce reasonable limits on names and counts.

### Versioning

- Requests include `version`. Servers MUST reject unsupported versions with an error.
- New ops or fields SHOULD be additive; when incompatible changes are required, bump `version`.

### Notes for Adapter Implementers

- WinFsp: `Control` requires METHOD_BUFFERED and a `0x8000` DeviceType; see `reference_projects/winfsp/inc/winfsp/winfsp.h` for details. Keep SSZ responses small.
- FUSE: consult `reference_projects/libfuse/example/ioctl.c` for a pattern; ensure the handler only applies to the control file inode. Use SSZ encoding/decoding.
- FSKit: model XPC after `FSKitSample` structure; route through the extension’s process, not the app UI. Use SSZ encoding/decoding.
