## AgentFS CLI — Snapshots and Branches (integrated into `ah`)

### Purpose

Specify a cross‑platform command‑line interface to:

- Create and list snapshots of an AgentFS volume
- Create branches (writable clones) from a given snapshot
- Bind a process (or launch a command) within a specific branch view

This CLI is integrated as subcommands of the main `ah` CLI. It controls the running user‑space filesystem server (adapter) hosting AgentFS Core. Delivery mechanisms and message schemas are specified in [AgentFS Control Messages](AgentFS%20Control%20Messages.md). Control is relayed using platform‑appropriate mechanisms validated by reference projects:

- Windows (WinFsp): DeviceIoControl to the mounted volume (maps to WinFsp `Control` entry‑point)
- Linux/macOS (FUSE): ioctl on a special control file in the mount or the file’s inode (maps to libfuse `ioctl` op)
- macOS (FSKit): XPC call to the FS extension; optional control file fallback within the mounted volume

### Command Overview

- ah agent fs snapshot create [--name <NAME>] --mount <MOUNT>
- ah agent fs snapshot list --mount <MOUNT>
- ah agent fs branch create --from <SNAPSHOT_ID> [--name <NAME>] --mount <MOUNT>
- ah agent fs branch bind --branch <BRANCH_ID> --mount <MOUNT> [--pid <PID>]
- ah agent fs branch exec --branch <BRANCH_ID> --mount <MOUNT> -- <COMMAND> [ARGS...]

Notes:

- SNAPSHOT_ID and BRANCH_ID are opaque identifiers returned by the server (ULID/UUID‑like).
- On Windows, <MOUNT> is the drive letter or volume path (e.g., X:). On FUSE/FSKit, <MOUNT> is the mount directory.

### Behavior and Core Mapping

- snapshot create: requests `FsCore::snapshot_create(name)` on the target volume; outputs `{ id, name }`.
- snapshot list: requests `FsCore::snapshot_list()`; outputs array of `{ id, name }`.
- branch create: requests `FsCore::branch_create_from_snapshot(snapshot_id, name)`; outputs `{ id, name, parent }`.
- branch bind: requests binding of the indicated PID (default: calling process) to the branch via `FsCore::bind_process_to_branch(branch_id)`; server associates the PID with the branch context.
- branch exec: convenience flow: bind current process to branch → exec COMMAND; server resolves branch by the caller’s PID for subsequent filesystem ops.

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
- Payloads: METHOD_BUFFERED; input/output are small JSON or packed structs (versioned). Example JSON payloads:
  - Snapshot create (in): `{ "op":"snapshot.create", "name":"<NAME>" }`
  - Snapshot create (out): `{ "id":"<ID>", "name":"<NAME>" }`
  - Branch create (in): `{ "op":"branch.create", "from":"<SNAPSHOT_ID>", "name":"<NAME>" }`
  - Branch bind (in): `{ "op":"branch.bind", "branch":"<BRANCH_ID>", "pid":<PID> }`
- JSON schemas:
  - Request: `specs/Public/Schemas/agentfs-control.request.schema.json`
  - Response: `specs/Public/Schemas/agentfs-control.response.schema.json`
- The adapter parses the JSON (or struct), calls the appropriate `FsCore` method, and fills the output buffer.

#### Linux/macOS (FUSE)

- Mechanism: libfuse `ioctl` on a special control file within the mount (common pattern; see libfuse ioctl example). The adapter exports `.agentfs/control` as a regular file that accepts ioctl.
- Client flow:
  - Open `<MOUNT>/.agentfs/control`
  - Call `ioctl(fd, AGENTFS_IOCTL_CMD, &buffer)` where `AGENTFS_IOCTL_CMD` is a private ioctl number; buffer is JSON or a compact struct.
- Supported operations mirror those on Windows:
  - snapshot.create, snapshot.list, branch.create, branch.bind
- Return values: success indicated by 0; results are copies into the user buffer; errors mapped to `-errno`.
- JSON schemas:
  - Request: `specs/Public/Schemas/agentfs-control.request.schema.json`
  - Response: `specs/Public/Schemas/agentfs-control.response.schema.json`

#### macOS (FSKit)

- Primary mechanism: XPC to the FS extension (recommended by FSKit); the extension exposes methods to handle `snapshot.create`, `snapshot.list`, `branch.create`, `branch.bind` and calls `FsCore`.
- Fallback: a control file under `<MOUNT>/.agentfs/control` that intercepts writes or ioctls (if supported) and executes commands (same as FUSE path). This path is useful for a single CLI that works with either FSKit or FUSE during development.
- JSON schemas:
  - Request: `specs/Public/Schemas/agentfs-control.request.schema.json`
  - Response: `specs/Public/Schemas/agentfs-control.response.schema.json`

### Error Handling

- Windows: NTSTATUS from adapter mapped to Win32 error for CLI; non‑zero exit code on failure; JSON `{"error":"..."}` as message when using stdout.
- FUSE/FSKit: adapter returns `-errno`; CLI maps to readable messages; exit non‑zero.

### Examples

- Create a snapshot with a name:
  - Windows: `ah agent fs snapshot create --mount X: --name clean`
  - FUSE: `ah agent fs snapshot create --mount /mnt/ah --name clean`

- List snapshots:
  - `ah agent fs snapshot list --mount /mnt/ah`

- Create a branch from snapshot and bind current shell:
  - `ah agent fs branch create --mount /mnt/ah --from 01HV... --name task-123 > branch.json`
  - `ah agent fs branch bind --mount /mnt/ah --branch $(jq -r .id branch.json)`

- Run a command in a branch:
  - `ah agent fs branch exec --mount /mnt/ah --branch 01HW... -- bash -lc "make test"`

### Security Considerations

- Only allow control from authenticated principals:
  - Windows: check caller token on DeviceIoControl (server side) and validate admin/user policy.
  - FUSE/FSKit: restrict `.agentfs/control` permissions (root/admin only) or enforce per‑user policy inside the adapter.
- Validate payloads; limit name lengths; sanitize JSON.

### Implementation Notes

- The JSON control format keeps the ABI stable and human‑inspectable. For performance, a packed C struct can be used; version fields are recommended.
- On Windows define IOCTL codes with `CTL_CODE(FILE_DEVICE_UNKNOWN | 0x8000, FUNCTION, METHOD_BUFFERED, FILE_ANY_ACCESS)`.
- On FUSE, implement `ioctl` handler in the adapter, and parse commands only when the path is `.agentfs/control`.
- On FSKit, expose an XPC interface from the extension target; the CLI uses a matching XPC client to send commands.
