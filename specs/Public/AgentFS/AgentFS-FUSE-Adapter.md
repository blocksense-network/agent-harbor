## AgentFS — FUSE Adapter (libfuse, high-level)

### Purpose

Implement a thin FUSE host that maps `struct fuse_operations` to AgentFS Core (`FsCore`) calls. Targets Linux/macOS (via libfuse or macFUSE during development). Aligns with libfuse semantics, caching knobs, readdir/readdir+ behavior, and optional advanced ops. Control plane delivery and message schemas are detailed in [AgentFS-Control-Messages](AgentFS-Control-Messages.md).

### References

- Header/API: `reference_projects/libfuse/include/fuse.h` (struct fuse_operations)
- Examples: `reference_projects/libfuse/example/{passthrough.c, passthrough_ll.c, hello.c}`

### Initialization

- Parse mount options; set `fuse_config` cache knobs based on `FsConfig.cache` (entry_timeout, negative_timeout, attr_timeout, direct_io/writeback, use_ino, readdir_ino).
- Provide private_data with pointer to adapter state including `FsCore` instance and branch binding rules.

### Operation Mapping (struct fuse_operations)

- getattr(path, struct stat\*, fi)
  - `FsCore::getattr(path)` → fill `stat` (mode, size, times, nlink). Respect `use_ino` if enabled.

- readlink(path, buf, size)
  - `FsCore::readlink(path)`; copy to buf (NUL‑terminated if space permits).

- mknod(path, mode, dev)
  - For regular files: `FsCore::create(path, &OpenOptions{create:true, ...})` then close. Character/block nodes typically unsupported; return `-ENOSYS` unless implemented.

- mkdir(path, mode)
  - `FsCore::mkdir(path, mode)`.

- unlink(path)
  - `FsCore::unlink(path)` (delete‑on‑close semantics handled by kernel/cache and core; adapter returns result).

- rmdir(path)
  - `FsCore::rmdir(path)` (implemented via check + unlink of directory entry).

- symlink(target, linkpath)
  - `FsCore::symlink(target, linkpath)`.

- rename(old, new, flags)
  - `FsCore::rename(old, new, flags & RENAME_NOREPLACE == 0)`; handle `RENAME_EXCHANGE` if supported or return `-ENOSYS`.

- link(existing, newlink)
  - `FsCore::link(existing, newlink)` if hardlinks supported; else `-ENOSYS`.

- chmod(path, mode, fi)
  - Map to attributes: set POSIX mode within core metadata.

- chown(path, uid, gid, fi)
  - If core tracks ownership, update; else synthesize or return `-ENOSYS`.

- truncate(path, off, fi)
  - If `fi` has a handle, use it; else open, truncate, close: `FsCore::truncate(h, off)`.

- open(path, fi)
  - Translate `fi->flags` (O_RDONLY/O_WRONLY/O_RDWR/O_APPEND) to `OpenOptions`; `FsCore::open` → `HandleId` set into `fi->fh`.

- read(path, buf, size, off, fi)
  - `FsCore::read(fi->fh, off, buf)`; return bytes read.

- write(path, data, size, off, fi)
  - `FsCore::write(fi->fh, off, data)`; handle O_APPEND via kernel when writeback enabled; otherwise compute EOF offset first.

- statfs(path, struct statvfs\*)
  - Fill from `FsCore::stats()` and limits/spill directory.

- flush(path, fi)
  - `FsCore::flush(fi->fh)` (note: may be called multiple times per open).

- release(path, fi)
  - `FsCore::close(fi->fh)`; clear handle.

- fsync(path, datasync, fi)
  - `FsCore::fsync(fi->fh, datasync!=0)`.

- setxattr/getxattr/listxattr/removexattr
  - `FsCore::xattr_set/get/list` and remove; map names directly.

- opendir(path, fi)
  - Optionally create a directory handle/iterator; else store sentinel.

- readdir(path, void \*buf, fuse_fill_dir_t filler, off_t off, fi, flags)
  - If flags includes `FUSE_READDIR_PLUS` and core supports readdir_plus: prefetch attrs.
  - Enumerate `FsCore::readdir(path)`; call `filler` with name, type, inode when `use_ino`.

- releasedir(path, fi)
  - Close any dir iterator context.

- fsyncdir(path, datasync, fi)
  - No‑op or flush directory metadata via core.

- access(path, mask)
  - If `default_permissions` not used, check via core metadata and current uid/gid from `fuse_context`.

- create(path, mode, fi)
  - `FsCore::create(path, &OpenOptions{create:true, ...})` → `fi->fh`.

- lock(path, fi, cmd, flock\*) / flock(path, fi, op)
  - Map to `FsCore::lock/unlock` (byte‑range for POSIX, whole‑file for flock).

- utimens(path, timespec[2], fi)
  - Translate to `FileTimes` and call `FsCore::set_times`.

- bmap(path, blocksize, \*idx)
  - Not applicable for non‑blkdev; `-ENOSYS`.

- ioctl(path, cmd, ...)
  - Optional: allow control messages (e.g., process‑branch binding) via simple IOCTL protocol; otherwise `-ENOTTY`.

- poll(path, fi, ph, reventsp)
  - Optional; if implemented, tie to core event subscriptions.

- write_buf/read_buf
  - Optional; can translate to core read/write with buffers to avoid extra copies where possible.

- fallocate(path, mode, off, len, fi)
  - `FsCore::fallocate(fi->fh, mode, off, len)` if implemented; else `-ENOSYS`.

- copy_file_range(path_in, fi_in, off_in, path_out, fi_out, off_out, size, flags)
  - `FsCore::copy_file_range(hin, off_in, hout, off_out, size)` if implemented; else `-ENOSYS`.

- lseek(path, off, whence, fi)
  - If supported, translate to querying file size and computing new offset; else `-ENOSYS`.

### Caching and Options

- Map `FsConfig.cache` to `fuse_config`:
  - `attr_timeout`, `entry_timeout`, `negative_timeout`, `use_ino`, `readdir_ino`.
  - `writeback_cache` and `direct_io` per core capability; ensure fsync correctness when writeback enabled.
  - When `auto_cache` is true, set to reflect changes immediately (passthrough guidance).

### Branch Binding

- Use `fuse_context` pid to resolve per‑process branch via adapter map. Expose a control (ioctl or a special file under `.agentfs`) to set the branch for the current process/session.
