## AgentFS — WinFsp Adapter

### Purpose

Implement a thin Windows adapter that maps WinFsp’s `FSP_FILE_SYSTEM_INTERFACE` operations to AgentFS Core (`FsCore`) calls. This enables mounting an AgentFS volume on Windows as a WinFsp user‑mode filesystem, honoring NT semantics (share modes, delete‑on‑close, security descriptors, reparse points, ADS). Control plane delivery and message schemas are detailed in [AgentFS-Control-Messages](AgentFS-Control-Messages.md).

### References

- Header: `reference_projects/winfsp/inc/winfsp/winfsp.h`
- Interface: `typedef struct _FSP_FILE_SYSTEM_INTERFACE { ... } FSP_FILE_SYSTEM_INTERFACE;`
- Example: `reference_projects/winfsp/tst/memfs/memfs.cpp`

### Initialization

- Create `FSP_FILE_SYSTEM_INTERFACE` and fill function pointers.
- Provide `FSP_FSCTL_VOLUME_PARAMS` (sector size, allocation unit, case sensitivity, features like `SupportsPosixUnlinkRename`).
- Create `FSP_FILE_SYSTEM` via `FspFileSystemCreate`, then `FspFileSystemStartDispatcher`.
- Maintain a mapping between WinFsp `FileContext` (`void*`) and AgentFS `HandleId`/node context.

### Operation Mapping (selected highlights)

Below is the WinFsp entry‑point list (from winfsp.h) and the corresponding AgentFS Core mapping. Only operations present in `FSP_FILE_SYSTEM_INTERFACE` are listed.

- GetVolumeInfo(FileSystem, VolumeInfo)
  - Map to `FsCore::stats()` and configuration (volume name, serial). Fill `FSP_FSCTL_VOLUME_INFO` accordingly (total/free space use spill dir capacity or configured values).

- SetVolumeLabel(FileSystem, VolumeLabel, VolumeInfo)
  - Store in adapter state; core does not persist labels. Update `VolumeInfo`.

- GetSecurityByName(FileSystem, FileName, PFileAttributes/ReparseIdx, SecurityDescriptor, PSecurityDescriptorSize)
  - Use `FsCore::getattr(path)` to fill attributes. For security descriptor: map simplified ACL from core config (owner SID, DACL based on POSIX bits) or return minimal descriptor. Reparse index unsupported unless symlink/reparse implemented; return default when not applicable.

- Create(FileSystem, FileName, CreateOptions, GrantedAccess, FileAttributes, SecurityDescriptor, AllocationSize, PFileContext, FileInfo)
  - Translate CreateOptions (FILE_DIRECTORY_FILE → mkdir path; else create file).
  - Map GrantedAccess to share modes and open flags; build `OpenOptions` accordingly.
  - For directories: `FsCore::mkdir(path, mode)`, then open dir handle if needed.
  - For files: `FsCore::create(path, &OpenOptions)` → `HandleId`; store `FileContext` with `HandleId` and branch binding.
  - Fill `FileInfo` from `FsCore::getattr(path)`.

- Open(FileSystem, FileName, CreateOptions, GrantedAccess, PFileContext, FileInfo)
  - `FsCore::open(path, &OpenOptions)` → `HandleId`; store in `FileContext`.
  - Fill `FileInfo` via `FsCore::getattr(path)`.

- Overwrite(FileSystem, FileContext, FileAttributes, ReplaceFileAttributes, AllocationSize, FileInfo)
  - If truncate: `FsCore::truncate(h, 0)` or set new length; update basic attributes via `set_times`/mode mapping.
  - Fill `FileInfo` post‑operation.

- Cleanup(FileSystem, FileContext, FileName, Flags)
  - If `Flags & FspCleanupDelete`: perform delete‑on‑close semantics:
    - If file previously unlinked via `SetDelete` or `unlink`, ensure removal after last `Close`.
    - If directory, ensure emptiness rules were enforced earlier.
  - Update times/attributes when `FspCleanupSet*` flags present.

- Close(FileSystem, FileContext)
  - `FsCore::close(HandleId)` and free adapter context. If pending delete‑on‑close and no more handles exist, finalize deletion (core already hides entry at unlink; here ensure storage reclamation semantics align).

- Read(FileSystem, FileContext, Buffer, Offset, Length, PBytesTransferred)
  - `FsCore::read(h, offset, buf)`.

- Write(FileSystem, FileContext, Buffer, Offset, Length, WriteToEndOfFile, ConstrainedIo, PBytesTransferred, FileInfo)
  - Compute target offset (EOF if requested), honor ConstrainedIo (no growth). `FsCore::write(h, off, data)`; update `FileInfo`.

- Flush(FileSystem, FileContext, FileInfo)
  - `FsCore::flush(h)`; `FileInfo` via getattr.

- GetFileInfo(FileSystem, FileContext, FileInfo)
  - `FsCore::getattr(path_or_handle)`; fill `FSP_FSCTL_FILE_INFO`.

- SetBasicInfo(FileSystem, FileContext, FileAttributes, CreationTime, LastAccessTime, LastWriteTime, ChangeTime, FileInfo)
  - Build `FileTimes` and apply via `FsCore::set_times(...)`. Map attributes to core mode flags where meaningful.

- SetFileSize(FileSystem, FileContext, NewSize, SetAllocationSize, FileInfo)
  - `FsCore::truncate(h, NewSize)`; update `FileInfo`.

- CanDelete(FileSystem, FileContext, FileName)
  - For files: ok. For directories: ensure empty via `readdir`.

- Rename(FileSystem, FileContext, FileName, NewFileName, ReplaceIfExists)
  - `FsCore::rename(from, to, replace)`.

- GetSecurity / SetSecurity(FileSystem, FileContext, ...)
  - Map to simplified ACL storage in adapter (optional). If not storing full ACLs, synthesize descriptors based on POSIX bits; accept SetSecurity and persist opaque blob or return unsupported if policy forbids.

- ReadDirectory(FileSystem, FileContext, Pattern, Marker, Buffer, Length, PBytesTransferred)
  - Use `FsCore::readdir(path)` or handle with iterator. Serialize entries with `FspFileSystemAddDirInfo` layout into Buffer; honor Marker for continuation.

- ResolveReparsePoints / GetReparsePoint / SetReparsePoint / DeleteReparsePoint
  - If symlinks are implemented: map to `FsCore::readlink/symlink`; otherwise return `STATUS_NOT_IMPLEMENTED` or appropriate reparse code. Do not hallucinate target formats.

- GetStreamInfo(FileSystem, FileContext, Buffer, Length, PBytesTransferred)
  - `FsCore::streams_list(path)`; serialize as `FILE_STREAM_INFORMATION` records.

- GetDirInfoByName(FileSystem, FileContext, FileName, DirInfo)
  - Lookup single child and fill `FSP_FSCTL_DIR_INFO` via `getattr`.

- Control(FileSystem, FileContext, ControlCode, ...)
  - Provide FSCTL for AgentFS‑specific controls if needed (e.g., bind process to branch). Map to `FsCore::bind_process_to_branch` by interpreting InputBuffer.

- SetDelete(FileSystem, FileContext, FileName, DeleteFile)
  - Mark for delete‑on‑close (adapter flag). On cleanup of last handle, call `FsCore::unlink(path)` if not already hidden.

- CreateEx / OverwriteEx / GetEa / SetEa
  - If EA supported: map to xattr via `xattr_*` on core. Otherwise return `STATUS_NOT_IMPLEMENTED`. Ensure CreateEx precedence over Create when present.

### Share Modes and Locks

- At `Create/Open`, enforce Windows share modes by consulting existing open handles tracked in adapter; deny when conflicting (FsCore maintains POSIX locks separately).
- Map `lock` operations via core byte‑range locks by offset/length.

### ADS (Alternate Data Streams)

- For path `name:stream`, populate `OpenOptions.stream` and forward to core. Enumerate via `GetStreamInfo`.

### Branch Binding

- Before launching an agent process, DeviceIoControl can pass a branch id; adapter calls `FsCore::bind_process_to_branch`. Each request uses calling PID to resolve branch context.
