## AgentFS CLI — Snapshots and Branches (integrated into `ah`)

### Purpose

Specify a cross‑platform command‑line interface to:

- Create and list snapshots of an AgentFS volume
- Create branches (writable clones) from a given snapshot
- Bind a process (or launch a command) within a specific branch view

This CLI is integrated as subcommands of the main `ah` CLI. It controls the running user‑space filesystem server (adapter) hosting AgentFS Core. Delivery mechanisms and message schemas are specified in [AgentFS-Control-Messages](AgentFS-Control-Messages.md). Control is relayed using platform‑appropriate mechanisms validated by reference projects:

- Windows (WinFsp): DeviceIoControl to the mounted volume (maps to WinFsp `Control` entry‑point)
- Linux/macOS (FUSE): ioctl on a special control file in the mount or the file’s inode (maps to libfuse `ioctl` op)
- macOS (FSKit): XPC call to the FS extension; optional control file fallback within the mounted volume

### Command Overview

ah agent fs snapshot create [--name <NAME>] --mount <MOUNT>
Create a new snapshot of the current branch state.

ah agent fs snapshot list --mount <MOUNT>
List all snapshots for the volume.

ah agent fs branch create --from <SNAPSHOT_ID> [--name <NAME>] --mount <MOUNT>
Create a new writable branch from the specified snapshot.

ah agent fs branch bind --branch <BRANCH_ID> --mount <MOUNT> [--pid <PID>]
Bind the current (or specified) process to a branch view.

ah agent fs branch exec --branch <BRANCH_ID> --mount <MOUNT> -- <COMMAND> [ARGS...]
Execute a command in the context of the specified branch.

ah agent fs backstore create-ramdisk --fs <FS> --size <MB> --mount <MOUNT>
Create a RAM disk with the specified filesystem and attach as backstore.

ah agent fs backstore attach --root <PATH> --mount <MOUNT>
Attach an existing host directory as the backstore root.

ah agent fs policy set --windows-open-redirect=<on|off> --mount <MOUNT>
Enable/disable Windows experimental open-redirect fast-path.

ah agent fs backstore status --mount <MOUNT>
Query the current backstore configuration and capabilities.

ah agent fs interpose set --forwarding=<eager_upperize|disabled> [--max-copy-bytes <BYTES>] [--require-reflink=<true|false>] --mount <MOUNT>
Configure interpose/FD-forwarding policy for the volume.

ah agent fs interpose get --mount <MOUNT>
Query the current interpose/FD-forwarding configuration.

ah agent fs policy get --mount <MOUNT>
Query the current policy settings for the volume.

ah agent fs snapshot copy-active --label <LABEL> --mount <MOUNT>
Create a snapshot by copying active upper entries (fallback for non-native backstores).

Notes:

- SNAPSHOT_ID and BRANCH_ID are opaque identifiers returned by the server (ULID/UUID‑like).
- On Windows, <MOUNT> is the drive letter or volume path (e.g., X:). On FUSE/FSKit, <MOUNT> is the mount directory.

### Behavior and Core Mapping

- snapshot create: requests `FsCore::snapshot_create(name)` on the target volume; outputs `{ id, name }`.
- snapshot list: requests `FsCore::snapshot_list()`; outputs array of `{ id, name }`.
- branch create: requests `FsCore::branch_create_from_snapshot(snapshot_id, name)`; outputs `{ id, name, parent }`.
- branch bind: requests binding of the indicated PID (default: calling process) to the branch via `FsCore::bind_process_to_branch(branch_id)`; server associates the PID with the branch context.
- branch exec: convenience flow: bind current process to branch → exec COMMAND; server resolves branch by the caller's PID for subsequent filesystem ops.
- backstore create-ramdisk: requests `backstore.create_ramdisk(fs, size_mb, opts)`; outputs `{ mount, supports_native_snapshots }`.
- backstore attach: requests `backstore.attach_hostfs(root)`; outputs `{ root }`.
- backstore status: requests `backstore.status()`; outputs `{ kind, root_or_mount, supports_native_snapshots }`.
- interpose set: requests `interpose.set(forwarding, max_copy_bytes?, require_reflink?)`; outputs success confirmation.
- interpose get: requests `interpose.get()`; outputs `{ forwarding, max_copy_bytes, require_reflink }`.
- policy set: requests `policy.set(windows: { open_redirect: bool })`; outputs success confirmation.
- policy get: requests `policy.get()`; outputs `{ windows: { open_redirect: bool } }`.
- snapshot native: requests `snapshot.native(label)`; outputs `{ id }` (if backstore supports native snapshots).
- snapshot copy-active: requests `snapshot.copy_active(label)`; outputs `{ id }` (fallback for non-native backstores).

### Transport Details by Platform

#### Windows (WinFsp)

- Mechanism: DeviceIoControl on a volume handle; handled by WinFsp `FSP_FILE_SYSTEM_INTERFACE::Control`.
- Handle acquisition: `CreateFile("\\\\.\\X:", GENERIC_READ|GENERIC_WRITE, FILE_SHARE_READ|FILE_SHARE_WRITE, ...)`.
- Control codes: Use custom IOCTLs with a user DeviceType (bit 0x8000) and METHOD_BUFFERED (per winfsp.h `Control` requirements):
  - IOCTL_AGENTFS_SNAPSHOT_CREATE
  - IOCTL_AGENTFS_SNAPSHOT_LIST
  - IOCTL_AGENTFS_BRANCH_CREATE
  - IOCTL_AGENTFS_BRANCH_BIND
  - IOCTL_AGENTFS_BRANCH_EXEC (optional; client can also do bind+CreateProcess)
  - IOCTL_AGENTFS_BACKSTORE_CREATE
  - IOCTL_AGENTFS_BACKSTORE_ATTACH
  - IOCTL_AGENTFS_POLICY_SET
  - IOCTL_AGENTFS_FD_OPEN
- Payloads: METHOD_BUFFERED; input/output are small SSZ-encoded messages. Versioning is handled via a message prefix: `(version: u16, length: u32, ssz_bytes)`. Messages are encoded using SSZ (Simple Serialize) format for compact, secure binary serialization.
- Logical Schemas (defining logical structure, SSZ is the wire format):
  - Request: `specs/Public/Schemas/agentfs-control.request.logical.json`
  - Response: `specs/Public/Schemas/agentfs-control.response.logical.json`
- The adapter parses the SSZ-encoded request, calls the appropriate `FsCore` method, and fills the output buffer with SSZ-encoded response.

#### Linux/macOS (FUSE)

- Mechanism: libfuse `ioctl` on a special control file within the mount (common pattern; see libfuse ioctl example). The adapter exports `.agentfs/control` as a regular file that accepts ioctl.
- Client flow:
  - Open `<MOUNT>/.agentfs/control`
  - Call `ioctl(fd, AGENTFS_IOCTL_CMD, &buffer)` where `AGENTFS_IOCTL_CMD` is a private ioctl number; buffer contains SSZ-encoded request.
- Supported operations mirror those on Windows:
  - snapshot.create, snapshot.list, snapshot.native, snapshot.copy_active, branch.create, branch.bind, backstore.create_ramdisk, backstore.attach_hostfs, backstore.status, interpose.set, interpose.get, policy.set, policy.get
- Return values: success indicated by 0; results are copies into the user buffer; errors mapped to `-errno`.
- Logical Schemas (defining logical structure, SSZ is the wire format):
  - Request: `specs/Public/Schemas/agentfs-control.request.logical.json`
  - Response: `specs/Public/Schemas/agentfs-control.response.logical.json`

#### macOS (FSKit)

- Primary mechanism: XPC to the FS extension (recommended by FSKit); the extension exposes methods to handle all control operations and calls `FsCore`.
- Fallback: a control file under `<MOUNT>/.agentfs/control` that intercepts writes or ioctls (if supported) and executes commands (same as FUSE path). This path is useful for a single CLI that works with either FSKit or FUSE during development.
- Logical Schemas (defining logical structure, SSZ is the wire format):
  - Request: `specs/Public/Schemas/agentfs-control.request.logical.json`
  - Response: `specs/Public/Schemas/agentfs-control.response.logical.json`

### Error Handling

- Windows: NTSTATUS from adapter mapped to Win32 error for CLI; non‑zero exit code on failure; JSON `{"error":"..."}` as message when using stdout.
- FUSE/FSKit: adapter returns `-errno`; CLI maps to readable messages; exit non‑zero.

### Examples

- Create a snapshot with a name:
  - Windows: `ah agent fs snapshot create --mount X: --name clean`
  - FUSE: `ah agent fs snapshot create --mount /mnt/aw --name clean`

- List snapshots:
  - `ah agent fs snapshot list --mount /mnt/aw`

- Create a branch from snapshot and bind current shell:
  - `ah agent fs branch create --mount /mnt/aw --from 01HV... --name task-123 > branch.json`
  - `ah agent fs branch bind --mount /mnt/aw --branch $(jq -r .id branch.json)`

- Run a command in a branch:
  - `ah agent fs branch exec --mount /mnt/aw --branch 01HW... -- bash -lc "make test"`

### Security Considerations

- Only allow control from authenticated principals:
  - Windows: check caller token on DeviceIoControl (server side) and validate admin/user policy.
  - FUSE/FSKit: restrict `.agentfs/control` permissions (root/admin only) or enforce per‑user policy inside the adapter.
- Validate SSZ payloads against schemas; limit name lengths and enforce reasonable limits on all fields.

### Implementation Notes

- The SSZ control format provides compact, secure binary serialization while keeping the ABI stable and verifiable. Version prefixes are required for compatibility.
- On Windows define IOCTL codes with `CTL_CODE(FILE_DEVICE_UNKNOWN | 0x8000, FUNCTION, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
- On FUSE, implement `ioctl` handler in the adapter, and parse commands only when the path is `.agentfs/control`.
- On FSKit, expose an XPC interface from the extension target; the CLI uses a matching XPC client to send commands.
