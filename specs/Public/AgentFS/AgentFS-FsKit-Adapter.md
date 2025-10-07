## AgentFS — FSKit Adapter (macOS)

### Purpose

Describe a thin adapter that maps Apple FSKit file system operations to AgentFS Core (`FsCore`) calls. FSKit is Apple’s user‑space filesystem framework (macOS 15+). This document focuses on the unary filesystem flow (FSUnaryFileSystem) as used in sample projects, mapping its key operations to core APIs. Control plane delivery and message schemas are detailed in [AgentFS-Control-Messages](AgentFS-Control-Messages.md).

### References

- FSKit concepts and sample: `reference_projects/FSKitSample/` and `fskit-docs/documentation/fskit/`
- Types: `FSUnaryFileSystem`, `FSUnaryFileSystemOperations`, `FSVolume`, `FSItem`, and protocol methods documented in the sample and docs set.

### Adapter Structure

- Implement an FSKit App Extension (`UnaryFileSystemExtension`) returning a custom class `AgentFsUnary` that conforms to `FSUnaryFileSystem & FSUnaryFileSystemOperations`.
- On `loadResource`, create and retain an `FsCore` instance configured from adapter settings and present an `FSVolume` implementation (`AgentFsVolume`).
- Propagate case‑insensitive‑preserving name policy by default (macOS convention), aligned with `FsConfig.case_sensitivity`.

### FSUnaryFileSystemOperations Mapping

- probeResource(resource, reply)

  - Determine supportability; reply with `.usable` result. No direct core call needed.

- loadResource(resource, options, reply)

  - Initialize `FsCore` with `FsConfig` and return `AgentFsVolume` instance.

- unloadResource(resource, options, reply)
  - Drop volume and release resources; allow core to be dropped.

### FSVolume Operations Mapping (AgentFsVolume)

Implement FSKit’s volume operations by delegating to `FsCore`. Concrete method names depend on FSKit SDK; based on the sample hierarchy, the following mappings apply:

- activate(options) -> FSItem (root)

  - Build an FSItem representing `/` using `FsCore::getattr("/")`.

- lookupItem(named: inDirectory:) -> FSItem

  - Resolve child path; use `FsCore::getattr(path)` to populate attributes; return item or error.

- createItem(named: type: inDirectory: ...) -> FSItem

  - If type directory: `FsCore::mkdir(path, mode)`
  - If type file: `FsCore::create(path, &OpenOptions{create:true})`, then close handle; return new FSItem.
  - If symlink: `FsCore::symlink(target, linkpath)`.

- removeItem(item:named:...) -> void

  - For file: `FsCore::unlink(path)`.
  - For dir: `FsCore::rmdir(path)`.

- enumerateDirectory(directory, ...) -> entries

  - `FsCore::readdir(path)`; return as FSKit directory entries with attributes when requested.

- Read/Write operations (FSVolume.ReadWriteOperations)

  - read(from:item:at:offset:length:buffer) → `FsCore::open` (if no handle), `FsCore::read`, `FsCore::close` (if transient).
  - write(contents:to:item:at:offset) → `FsCore::write` on an open handle; handle truncation when needed via `FsCore::truncate`.

- Xattrs (FSVolume.XattrOperations)

  - xattr(named:of:) → `FsCore::xattr_get`
  - setXattr(named:to:...) → `FsCore::xattr_set`
  - xattrs(of:) → `FsCore::xattr_list`

- Item attribute updates
  - chmod/chown equivalents map to updating core’s mode/ownership fields.
  - utimens → `FsCore::set_times` with `FileTimes`.

### FSVolume Read/Write Mapping (KBP behavior)
- **Read**:
  - If no upper entry, open **lower RO** handle and service reads; **do not** create upper node.
  - If an upper entry exists, read via **upper backstore** handle.
- **Write**:
  - On first mutation, perform **copy-up**, open **upper RW** handle, then write.
**Note:** FSKit does **not** support cross-FS handle splicing; data page-ins/page-outs still traverse this adapter.

### Metadata initialization
- On copy-up, initialize mode/uid/gid/ACL/xattrs from the **lower** entry per `copyup.*` policy; apply umask if configured.

### Case Sensitivity and Names

- Default to case‑insensitive‑preserving; configure `FsConfig.case_sensitivity` accordingly.
- FSKit presents names as `FSFileName`; adapter converts to UTF‑8 paths and calls core.

### Control Plane

The FSKit adapter exposes an XPC service for control operations. The adapter implements an XPC service that external processes can connect to for performing AgentFS control operations like snapshot creation, branch management, and process binding.

The XPC service name is `com.agentfs.AgentFSKitExtension.control` and implements the `AgentFSControlProtocol` with methods for:

- Creating snapshots
- Creating branches from snapshots
- Binding processes to branches
- Listing snapshots and branches

External tools and the `ah agent fs` CLI connect to this XPC service to perform control operations on the mounted filesystem.

### Notes and Limits

- FSKit exact method signatures are determined by Apple’s SDK; the above mappings follow the sample (`MyFS`, `MyFSVolume`, `MyFSItem`) operations shown in the reference project. Implementations should adhere to the SDK contracts without inventing additional entry points.
- Device files and block mappings are out of scope.
- For symlinks: implement via core `symlink`/`readlink` methods if the FSKit API exposes them; otherwise treat as regular items with a symlink attribute.
