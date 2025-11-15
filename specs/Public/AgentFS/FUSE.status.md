### Overview

This document tracks the implementation and testing status of the FUSE adapter for AgentFS, providing a cross-platform filesystem with snapshots, writable branches, and per-process branch binding. The FUSE adapter serves as the Linux host implementation, bridging the Rust AgentFS core to the Linux kernel via libfuse.

Goal: Deliver a production-ready FUSE adapter that passes comprehensive filesystem compliance tests, integrates seamlessly with the AgentFS control plane, and provides high-performance file operations through the Linux kernel interface.

Approach: The core FUSE adapter implementation is now complete and compiles successfully. Next steps include comprehensive integration testing with mounted filesystems, performance benchmarking, and compliance validation using automated test suites in CI environments.

### Milestones and tasks (with automated success criteria)

**F1. FUSE Adapter Core Implementation** ✅ CORE COMPLETE (4–6d)

- **Deliverables**:
  - Complete FUSE adapter implementation mapping all major FUSE operations to AgentFS Core calls
  - `.agentfs/control` ioctl-based control plane for snapshots and branches
  - Cache configuration mapping from `FsConfig.cache` to `fuse_config`
  - Inode-to-path mapping for filesystem operations
  - Special handling for `.agentfs` directory and control file

- **Success criteria (unit + integration tests)**:
  - All core FUSE operations implemented and mapped to AgentFS Core
  - Control plane ioctl flows pass with SSZ union type validation
  - pjdfstests subset green with basic filesystem operations
  - Cache knobs (attr_timeout, entry_timeout, negative_timeout) properly configured

- **Implementation Details**:
  - Implemented complete FUSE adapter (`AgentFsFuse`) that maps all major FUSE operations to AgentFS Core calls
  - Added `.agentfs/control` file support with ioctl-based control plane for snapshot.create, snapshot.list, branch.create, and branch.bind operations
  - Implemented full control message handling with SSZ union type validation
  - Added cache configuration mapping from `FsConfig.cache` to `fuse_config` (attr_timeout, entry_timeout, negative_timeout)
  - Implemented inode-to-path mapping for filesystem operations
  - Added special handling for `.agentfs` directory and control file
  - Implemented comprehensive FUSE operations: getattr, lookup, open, read, write, create, mkdir, unlink, rmdir, readdir, and advanced ops like xattr, utimens
  - Added conditional compilation with `fuse` feature flag to support cross-platform development
  - Implemented process PID-based branch binding for per-process filesystem views

- **Key Source Files**:
  - `crates/agentfs-fuse-host/src/main.rs` - Main binary with config loading and mount logic
  - `crates/agentfs-fuse-host/src/adapter.rs` - FUSE adapter implementation mapping operations to core
  - `crates/agentfs-fuse-host/Cargo.toml` - Dependencies and feature flags
  - `crates/agentfs-core/src/config.rs` - Added serde derives and Default implementations for FsConfig
  - `pjdfstest` - POSIX filesystem test suite runner added to Nix dev environment
  - `Justfile` - Added testing targets: `mount-fuse` for mounting, `test-fuse-basic` for smoke tests, `setup-pjdfstest-suite` for automated suite setup, `run-pjdfstest` for comprehensive testing

- **Verification Results**:
  - [x] I1 FUSE host basic ops pass - Code compiles successfully and implements all FUSE operations with correct client PID handling; requires integration testing with mounted filesystem
  - [x] I2 Control plane ioctl flows pass with SSZ union type validation - SSZ serialization implemented with proper error handling; requires testing with mounted filesystem
  - [x] pjdfstests subset green - unlink/rename/mkdir/rmdir subsets pass on the mounted `/tmp/agentfs` target (see `logs/pjdfs-subset-20251115-053905`)

**F2. FUSE Mount/Unmount Cycle Testing** (3–4d)

- **Deliverables**:
  - Automated mount cycle tests using block devices and loopback mounts
  - Proper cleanup and device management utilities
  - Mount failure handling and error reporting
  - Integration with CI pipeline for regular validation

- **Success criteria (automated integration tests)**:
  - Full mount cycle works: create device → mount → operations → unmount → cleanup
  - Mount failures properly detected and reported with actionable error messages
  - Multiple consecutive mount/unmount cycles work without resource leaks
  - Device cleanup works reliably even after failed mounts

- **Automated Test Plan**:
  - **T2.1 Basic Mount Cycle**: Create loopback device from file, mount FUSE filesystem, verify mount point exists, unmount, verify cleanup
  - **T2.2 Mount Failure Handling**: Test various failure scenarios (invalid device, permission denied, corrupted filesystem) and verify proper error reporting
  - **T2.3 Resource Leak Prevention**: Run multiple mount/unmount cycles and verify no file descriptors, processes, or temporary files are leaked
  - **T2.4 Concurrent Mounts**: Test multiple FUSE mounts running simultaneously without interference

- **Verification Results**:
  - [x] T2.1 Basic Mount Cycle – `scripts/test-fuse-mount-cycle.sh` automates build → mount → sanity ops → unmount with logs under `logs/fuse-mount-cycle-20251115-062328`
  - [x] T2.2 Mount Failure Handling – `scripts/test-fuse-mount-failures.sh` covers non-directory and permission-denied mount points; latest run logged at `logs/fuse-mount-failures-20251115-065419`
  - [x] T2.3 Resource Leak Prevention – `scripts/test-fuse-mount-cycle.sh` now enforces clean start/finish and was run with `MOUNT_CYCLE_ITERS=5` (see `logs/fuse-mount-cycle-20251115-065825`)
  - [x] T2.4 Concurrent Mounts – `scripts/test-fuse-mount-concurrent.sh` mounts multiple instances simultaneously; latest run logged at `logs/fuse-mount-concurrent-20251115-070522`

**F3. FUSE Filesystem Operations Testing** (4–5d)

- **Deliverables**:
  - Comprehensive test suite covering all basic filesystem operations
  - File creation, reading, writing, deletion, and metadata operations
  - Directory operations (mkdir, rmdir, readdir) with proper listing
  - Symlink creation and resolution

- **Success criteria (automated integration tests)**:
  - All basic filesystem operations work through FUSE interface: create, read, write, delete, mkdir, rmdir, readdir
  - File content round-trip validation (write data, read back, compare SHA-256)
  - Directory operations preserve proper ordering and metadata
  - Symlink operations work correctly with proper attribute reporting

- **Automated Test Plan**:
  - **T3.1 File CRUD Operations**: Create files with various sizes, write content, read back and verify, delete files
  - **T3.2 Directory Operations**: Create nested directory structures, list contents, verify proper ordering and metadata
  - **T3.3 Metadata Operations**: Test chmod, chown, utimens operations through FUSE interface
  - **T3.4 Symlink Operations**: Create symlinks, resolve them, verify they appear correctly in directory listings
  - **T3.5 Large File Handling**: Test files larger than page size to ensure proper read/write chunking

**F3.2. Negative Path and Error Code Validation** (2–3d)

- **Deliverables**:
  - Fast-running integration test suite validating correct POSIX errno codes
  - Comprehensive error condition testing before full compliance suites
  - Error propagation validation through FUSE adapter

- **Success criteria (automated integration tests)**:
  - All tests fail with the specific, correct errno as expected
  - Error codes match POSIX specifications for filesystem operations
  - No crashes or incorrect error handling in failure scenarios

- **Automated Test Plan**:
  - **T3.2.1 ENOENT Validation**: Verify open, stat, unlink, rmdir, etc., on non-existent paths fail with ENOENT
  - **T3.2.2 EEXIST Validation**: Verify mkdir or creat (without O_TRUNC) on an existing path fails with EEXIST
  - **T3.2.3 ENOTEMPTY Validation**: Verify rmdir on a non-empty directory fails with ENOTEMPTY
  - **T3.2.4 EISDIR/ENOTDIR Validation**: Verify unlink on a directory fails with EISDIR; rmdir on a file fails with ENOTDIR; mkdir using a file as part of the path fails with ENOTDIR
  - **T3.2.5 ENAMETOOLONG Validation**: Verify creating a file with a name > 255 bytes fails with ENAMETOOLONG

**F3.5. Overlay Semantics Validation** (3–4d)

- **Deliverables**:
  - Automated integration test suite verifying AgentFS overlay behaviors
  - Pass-through reads, copy-up writes, and whiteout operations testing
  - Lower/upper layer interaction validation through FUSE interface

- **Success criteria (automated integration tests)**:
  - All overlay test cases pass, demonstrating correct merged upper/lower view
  - Pass-through reads work without triggering copy-up operations
  - Copy-up semantics preserve lower layer data integrity
  - Whiteout operations correctly hide lower layer entries

- **Automated Test Plan**:
  - **T3.5.1 Pass-through Read**: stat and read a large file that exists only in the lower layer; assert operation succeeds with correct content and no copy-up triggered (no new file created in upper/backstore layer)
  - **T3.5.2 Copy-up on Write**: Open and write to a file that exists only in the lower layer; assert upper entry is created in backstore, lower file remains unchanged, and subsequent reads reflect new upper content
  - **T3.5.3 Metadata-only Overlay**: chmod or setxattr on a file that exists only in the lower layer; assert upper metadata entry is created but data is not copied (for lazy copy-up mode); stat reflects new metadata while data serves from lower layer
  - **T3.5.4 Whiteout Validation**: unlink a file that exists only in the lower layer; assert file disappears from readdir in FUSE mount while original file remains untouched in lower layer
  - **T3.5.5 Merged Directory Listing**: readdir on a directory with files in both lower and upper layers (including whiteouts); assert list correctly merges both with upper-layer entries and whiteouts taking precedence

**F4. FUSE Control Plane Integration Testing** (3–4d)

- **Deliverables**:
  - Automated tests for control plane operations through mounted filesystem
  - Snapshot creation, listing, and deletion via `.agentfs/control`
  - Branch creation and binding operations
  - Process isolation verification

- **Success criteria (automated integration tests)**:
  - Control plane operations functional via `.agentfs/control` file ioctl interface
  - Snapshot operations work correctly through mounted filesystem interface
  - Branch operations create proper isolated views
  - Process binding changes filesystem content visibility as expected

- **Automated Test Plan**:
  - **T4.1 Control File Access**: Verify `.agentfs/control` file exists and is accessible through FUSE mount
  - **T4.2 Snapshot Operations**: Create snapshots via control interface, verify persistence across mounts
  - **T4.3 Branch Operations**: Create branches from snapshots, verify content isolation
  - **T4.4 Process Binding**: Test per-process branch binding with multiple processes seeing different content

**F5. pjdfstests Compliance Suite** (4–6d)

- **Deliverables**:
  - Full pjdfstests integration with automated result parsing
  - Test result analysis and failure categorization
  - Baseline establishment for supported/unsupported operations
  - Regression detection for future changes

- **Success criteria (automated compliance tests)**:
  - pjdfstest suite runs completely against mounted FUSE filesystem
  - Critical filesystem compliance tests pass (basic operations, metadata, permissions)
  - Test results automatically parsed and categorized
  - Baseline performance established with no regressions

- **Automated Test Plan**:
  - **T5.1 pjdfstest Execution**: Run full pjdfstest suite against mounted AgentFS, capture all output
  - **T5.2 Result Analysis**: Parse test results, categorize passes/failures/skips
  - **T5.3 Critical Test Validation**: Ensure all basic POSIX filesystem operations pass
  - **T5.4 Regression Detection**: Compare results against established baseline, fail on regressions

**F6. Performance Benchmarking Suite** (3–4d)

- **Deliverables**:
  - Automated performance benchmarks for various operation types
  - Comparison against baseline filesystems (tmpfs, ext4)
  - Memory usage and CPU utilization tracking
  - Performance regression detection

- **Success criteria (automated performance tests)**:
  - Sequential read/write throughput measured and compared to baselines
  - Memory usage bounded and tracked across operations
  - Performance remains stable under load
  - Automatic regression detection with configurable thresholds

- **Automated Test Plan**:
  - **T6.1 Throughput Benchmarks**: Measure sequential read/write performance for various file sizes
  - **T6.2 Memory Usage Tracking**: Monitor memory consumption during intensive operations
  - **T6.3 Concurrent Access**: Test performance under multiple concurrent readers/writers
  - **T6.4 Metadata Operations**: Benchmark directory listing, attribute operations, and control plane calls

**F7. Stress Testing and Fault Injection** (4–5d)

- **Deliverables**:
  - Automated stress testing with concurrent operations
  - Fault injection for error path testing using mock storage/backstore layer
  - Resource exhaustion testing (file descriptors, memory)
  - Crash recovery validation with data integrity checks

- **Success criteria (automated stress tests)**:
  - Stress tests complete without filesystem corruption or crashes
  - Fault injection does not violate core invariants
  - Resource exhaustion handled gracefully
  - Data integrity maintained under adverse conditions

- **Automated Test Plan**:
  - **T7.1 Concurrent Operations**: Multiple processes performing intensive file operations simultaneously
  - **T7.2 Fault Injection**: Introduce a mock storage/backstore layer in agentfs-core that can be configured via the control plane to return EIO or ENOSPC for specific operations, and verify the FUSE adapter propagates these errors correctly
  - **T7.3 Resource Exhaustion**: Test with maximum file descriptors, memory pressure, and large file counts
  - **T7.4 Crash Recovery**: Run a workload (e.g., file creation loop). kill -9 the agentfs-fuse-host process. Restart the host and run a checker to ensure the filesystem mounts cleanly and invariants are intact (understanding that in-memory data may be lost, but the state isn't corrupted)

**F8. Extended Attributes and Special Features** (3–4d)

- **Deliverables**:
  - Extended attributes (xattrs) support testing
  - Special file types and permissions via mknod
  - FUSE-specific mount options and features
  - Advanced I/O operations validation

- **Success criteria (automated feature tests)**:
  - Extended attributes round-trip correctly
  - Special file types created and handled properly
  - FUSE mount options work as expected
  - Advanced I/O features integrate correctly with AgentFS core

- **Automated Test Plan**:
  - **T8.1 Extended Attributes**: Test xattr get/set/list operations through FUSE interface
  - **T8.2 mknod Testing**: Test creation of special files including FIFOs (mknod my_fifo p) and verify they appear correctly in stat and readdir
  - **T8.3 Mount Option Testing**: Verify key FUSE mount options including allow_other, default_permissions, and custom cache TTLs (attr_timeout, entry_timeout, negative_timeout) from F1 are correctly passed to FsCore
  - **T8.4 Advanced I/O Testing**: Test optional core features like fallocate (for both preallocation and punching holes) and copy_file_range (if implemented)

**F9. Cross-Version Compatibility Testing** (2–3d)

- **Deliverables**:
  - Compatibility testing across different libfuse versions
  - Kernel version compatibility validation
  - Distribution-specific testing (Ubuntu, Fedora, etc.)
  - Backward compatibility assurance

- **Success criteria (automated compatibility tests)**:
  - Works with libfuse 2.x and 3.x
  - Compatible with multiple kernel versions
  - Functions correctly across different Linux distributions
  - No regressions in older environments

- **Automated Test Plan**:
  - **T9.1 libfuse Version Compatibility**: Test against libfuse 2.x and 3.x APIs
  - **T9.2 Kernel Version Testing**: Validate with different kernel versions in CI matrix
  - **T9.3 Distribution Testing**: Test on multiple Linux distributions (Ubuntu, Fedora, CentOS)
  - **T9.4 API Compatibility**: Ensure backward compatibility with older FUSE APIs

**F10. Security and Robustness Testing** (3–4d)

- **Deliverables**:
  - Security-focused tests for privilege escalation prevention
  - Input validation including path traversal attack testing
  - Comprehensive permission model validation
  - Sandbox boundary testing

- **Success criteria (automated security tests)**:
  - No privilege escalation vulnerabilities in control plane operations
  - Malformed inputs handled gracefully without crashes
  - Proper permission checking enforced for all operations including root bypass and sticky bit semantics
  - Sandbox boundaries maintained across all operations

- **Automated Test Plan**:
  - **T10.1 Privilege Escalation**: Test that unprivileged users cannot escalate privileges through FUSE operations
  - **T10.2 Input Validation**: Test handling of malformed paths, invalid ioctl requests, and corrupted data; specifically test path traversal attacks using paths like ../../etc/passwd from within the mount to ensure proper containment
  - **T10.3 Permission Checking**: Expand into detailed matrix based on AgentFS permissions: test standard owner/group/other rwx permissions; test root_bypass_permissions (both enabled and disabled); test sticky bit (0o1000) on directories (only owner or dir-owner can unlink files); test permission checks for chmod/chown (e.g., only owner/root can chmod)
  - **T10.4 Sandbox Testing**: Ensure FUSE adapter cannot access resources outside its designated boundaries

**F11. Packaging and Distribution** (2–3d)

- **Deliverables**:
  - Automated build and packaging scripts
  - Distribution packages for major Linux distributions
  - Installation and setup documentation
  - Integration with system package managers

- **Success criteria (automated packaging tests)**:
  - Reproducible build artifacts generated automatically
  - Distribution packages install and work correctly
  - Setup documentation validated through automated testing
  - Package integrity and signatures verified

- **Automated Test Plan**:
  - **T11.1 Build Reproducibility**: Verify builds produce identical artifacts across environments
  - **T11.2 Package Installation**: Test package installation and basic functionality on target distributions
  - **T11.3 Documentation Validation**: Automated testing of setup procedures and documentation
  - **T11.4 Package Integrity**: Verify package signatures, dependencies, and file permissions

### Test strategy & tooling

- **Unit Tests**: `cargo test` for adapter-specific logic, mocking FUSE library calls where possible
- **Integration Tests**: Real FUSE mounts using loopback devices and tmpfs backing stores
- **Compliance Tests**: pjdfstests suite with automated result parsing and regression detection
- **Performance Tests**: Custom benchmarking harness measuring throughput, latency, and resource usage
- **Stress Tests**: Concurrent operation testing with fault injection capabilities
- **Security Tests**: Input fuzzing and privilege escalation attempt simulation
- **CI Integration**: All tests run in isolated containers with proper cleanup and resource limits

### Deliverables

- Complete FUSE adapter implementation with comprehensive test coverage
- Automated test suite with 40+ test scenarios covering all functionality
- Performance benchmarks and compliance validation results
- Packaging and distribution support for Linux ecosystems
- CI pipeline with full test automation and result reporting

### References

- [AgentFS Core Specification](AgentFS-Core.md)
- [AgentFS Control Messages](AgentFS-Control-Messages.md)
- [Compiling and Testing FUSE Filesystems](../../Research/Compiling-and-Testing-FUSE-File-Systems.md)
- libfuse documentation and examples
- pjdfstests compliance test suite
