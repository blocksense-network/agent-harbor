### Overview

This document addresses the comprehensive FSKit API review feedback provided by a senior developer experienced with macOS filesystem extensions. The review identified six critical correctness and security issues in the current FSKit adapter implementation that violate FSKit contracts and could cause data corruption or security vulnerabilities.

**Key Issues Identified:**
1. Caller identity using extension process instead of actual caller (security/correctness bug)
2. Name handling violating byte-level FSKit contract (potential data corruption)
3. Single-handle per item instead of per-open instance tracking (breaks concurrent access)
4. Read/write operations requiring pre-opened handles (fails kernel readahead/caching)
5. Extended attributes with fixed buffers and unrealistic PathConf limits
6. Rename operations ignoring `overItem` replace semantics

**Approach:** Implement the reviewer's provided patches in granular milestones with comprehensive automated testing. Each milestone addresses one or more issues with end-to-end integration tests that validate real filesystem behavior through the FSKit interface.

**Priority:** Address issues 1-4 (caller identity, names, handles, lazy I/O) first as they represent the most severe correctness and security risks.

### Milestones and tasks (with automated success criteria)

**M-Review.1. Implement Byte-Safe Name Handling** (3–4d)

- **Deliverables:**
  - Add `pathBytes: Data` field to `AgentFsItem` for byte-safe path storage
  - Implement `constructPathBytes()` and `withNullTerminatedCStr()` helpers
  - Replace all `FSFileName.string` usage in path construction with byte-safe operations
  - Update path building in lookup, create, remove, rename, symlink, and xattr operations

- **Verification:** Integration tests that create files with non-UTF-8 names (using raw bytes) and validate:
  - [ ] `ls` command displays correct names without corruption
  - [ ] `cp` and `mv` operations preserve byte-identical names
  - [ ] Finder/XCode don't crash when listing directories with non-UTF-8 names
  - [ ] Round-trip create/list/delete works for files with names containing null bytes or invalid UTF-8 sequences

**Implementation Details:**
- Follow reviewer's Patch 1: Add `pathBytes` field alongside existing `path` string (kept for debug logging only)
- Implement byte-safe path construction without intermediate String conversions
- Update all FFI calls to use `withNullTerminatedCStr(pathBytes)` instead of `path.withCString`

**Key Source Files:**
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsItem.swift` - Add pathBytes field
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift` - Path helpers and conversion

**M-Review.2. Fix Per-Open Handle Tracking** (2–3d)

- **Deliverables:**
  - Replace single `userData` handle with `opensByItem` dictionary mapping `FSItem.Identifier` to arrays of handles
  - Update `openItem()` to append handles to per-item arrays instead of overwriting `userData`
  - Update `closeItem()` to remove specific handles and maintain reference counting
  - Keep `userData` as fallback for legacy code paths during transition

- **Verification:** Concurrent access integration tests:
  - [ ] Multiple processes opening same file simultaneously succeed
  - [ ] Each process sees independent file handles with correct positioning
  - [ ] Closing one handle doesn't affect other concurrent opens
  - [ ] Handle reference counting prevents premature cleanup

**Implementation Details:**
- Follow reviewer's Patch 2: Add `opensByItem` with NSLock protection
- Track multiple handles per FSItem instead of single userData slot
- Update reclaim logic to clean up specific handles from arrays

**Key Source Files:**
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift` - Handle tracking implementation

**M-Review.3. Implement Lazy I/O Opens** (2–3d)

- **Deliverables:**
  - Modify `read()` and `write()` operations to transparently open transient handles when no existing handle is available
  - Implement read-only vs read-write transient open logic based on operation type
  - Ensure transient handles are properly closed after I/O completion
  - Maintain existing behavior when explicit handles are already open

- **Verification:** Kernel I/O integration tests:
  - [ ] `cat` command works on files without prior explicit open (kernel readahead path)
  - [ ] `dd` operations succeed without requiring application-level file opens
  - [ ] File caching and prefetch operations work correctly
  - [ ] No handle leaks from transient opens (validate with handle counting)

**Implementation Details:**
- Follow reviewer's Patch 2: Add transient handle resolution in read/write operations
- Check existing handles first, fall back to `af_open_by_id` with appropriate flags
- Ensure transient handles are closed even on error paths

**Key Source Files:**
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift` - Lazy open implementation in ReadWriteOperations

**M-Review.4. Fix Extended Attributes Implementation** (2–3d)

- **Deliverables:**
  - Implement dynamic buffer sizing for xattr operations (grow to actual size instead of fixed 4K)
  - Update PathConf to report realistic `maximumXattrSize` (65,536 bytes) instead of `Int.max`
  - Ensure byte-safe path handling in xattr operations
  - Maintain backward compatibility with existing xattr usage patterns

- **Verification:** Extended attributes integration tests:
  - [ ] Xattrs larger than 4K bytes round-trip correctly without truncation
  - [ ] `xattr -w` and `xattr -l` commands work with various payload sizes
  - [ ] macOS quarantine attributes (`com.apple.quarantine`) function properly
  - [ ] Finder metadata attributes work correctly

**Implementation Details:**
- Follow reviewer's Patch 2: Replace fixed 4K buffers with dynamic sizing loops
- First call with NULL buffer to get size, then allocate and retry
- Update PathConf to finite limits matching macOS conventions

**Key Source Files:**
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift` - Xattr operations and PathConf

**M-Review.5. Fix Rename with overItem Semantics** (2–3d)

- **Deliverables:**
  - Implement proper `overItem` handling in rename operations
  - When `overItem == nil`, fail with `EEXIST` if destination exists (no-replace semantics)
  - When `overItem != nil`, implement atomic replace or best-effort unlink-then-rename
  - Ensure byte-safe path handling in rename operations

- **Verification:** File operation integration tests:
  - [ ] `mv file1 file2` fails when file2 exists (no overwrite flag)
  - [ ] `mv file1 file2` succeeds when file2 exists (with overwrite flag)
  - [ ] Cross-directory renames work correctly with both replace and no-replace cases
  - [ ] Atomicity preserved where possible (no partial rename states)

**Implementation Details:**
- Follow reviewer's Patch 2: Add overItem checking and EEXIST handling
- Implement unlink-then-rename fallback for replace operations
- Note: Future FFI enhancement (`af_rename_replace`) could make this fully atomic

**Key Source Files:**
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift` - Rename operation implementation

**M-Review.6. Update PathConf Realistic Limits** (1–2d)

- **Deliverables:**
  - Set `maximumNameLength` to finite value (255 bytes) instead of -1
  - Set `maximumLinkCount` to reasonable finite value (65,535) instead of -1
  - Maintain existing `maximumFileSize` and `maximumXattrSize` values
  - Ensure all limits align with macOS filesystem conventions

- **Verification:** Filesystem limit validation tests:
  - [ ] PathConf reporting matches actual filesystem capabilities
  - [ ] Upper layers correctly handle finite limits
  - [ ] No spurious retries or failures due to mismatched expectations

**Implementation Details:**
- Follow reviewer's Patch 2: Replace unlimited (-1) values with conservative finite limits
- Values chosen to match typical macOS filesystem behavior

**Key Source Files:**
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift` - PathConf property implementations

**M-Review.7. Implement Proper Caller Identity (Audit Token)** (4–5d)

- **Deliverables:**
  - Extract caller's audit token from FSKit operation context instead of using extension process identity
  - Implement audit token to registered PID mapping for permission evaluation
  - Update all FFI calls to use caller's identity instead of extension identity
  - Maintain handle-to-PID caching for consistency across operations

- **Verification:** Security and permission integration tests:
  - [ ] File access respects actual caller permissions, not extension permissions
  - [ ] Permission denied errors occur for operations caller cannot perform
  - [ ] Cross-process operations use correct security context
  - [ ] Audit logs show operations attributed to correct processes

**Implementation Details:**
- Extract audit token from FSKit's per-operation context (reviewer's note about missing this in current code)
- Implement mapping from audit tokens to registered PIDs
- Update `getCallingPid()` and related functions throughout adapter

**Key Source Files:**
- `adapters/macos/xcode/AgentFSKitExtension/AgentFSKitExtension/AgentFsVolume.swift` - Caller identity extraction and mapping

**M-Review.8. Integration Testing Suite for Review Fixes** (3–4d)

- **Deliverables:**
  - Automated test suite that exercises all fixed functionality through real FSKit interface
  - Tests for non-UTF-8 filenames, concurrent opens, lazy I/O, large xattrs, rename semantics
  - Integration with existing E2E test framework (`e2e-fskit` Just target)
  - Test logs capture full output for failure analysis

- **Verification:** Comprehensive FSKit compliance tests:
  - [ ] All review-identified issues validated with automated tests
  - [ ] Test suite passes on clean macOS 15.4+ environment with SIP disabled
  - [ ] Each test generates unique log file with full output on failure
  - [ ] Tests integrate with existing CI pipeline

**Implementation Details:**
- Extend existing `tests/tools/e2e_macos_fskit/` with additional test scenarios
- Add non-UTF-8 filename generation and validation
- Test concurrent access patterns and lazy I/O scenarios
- Validate xattr operations with various sizes and Finder metadata

**Key Source Files:**
- `tests/tools/e2e_macos_fskit/` - Enhanced test suite
- `scripts/e2e-fskit.sh` - Updated test runner

**M-Review.9. Performance and Regression Testing** (2–3d)

- **Deliverables:**
  - Performance benchmarks comparing before/after review fixes
  - Regression tests ensuring fixes don't break existing functionality
  - Memory usage validation for new handle tracking and byte operations
  - Stress testing with concurrent operations

- **Verification:** Performance and stability tests:
  - [ ] No performance regression from byte-safe operations
  - [ ] Memory usage remains bounded with per-open handle tracking
  - [ ] Concurrent operation throughput maintained or improved
  - [ ] All existing functionality continues to work

**Implementation Details:**
- Benchmark path construction performance (bytes vs strings)
- Memory profiling of handle tracking under load
- Regression test suite covering all existing operations

**M-Review.10. Documentation and Compliance Validation** (2–3d)

- **Deliverables:**
  - Update implementation documentation with review findings and fixes
  - Document FSKit compliance improvements and remaining limitations
  - Update code comments explaining byte-safe operations and caller identity handling
  - Document test scenarios for validating fixes

- **Verification:** Documentation completeness checks:
  - [ ] All review issues documented with before/after state
  - [ ] Code comments explain non-obvious FSKit requirements
  - [ ] Implementation progress updated with review resolution status
  - [ ] Documentation passes linting and link checking

### Test strategy & tooling

- **Integration Tests:** Primary validation through FSKit interface using real filesystem operations (`e2e-fskit` target)
- **Security Tests:** Validate caller identity and permission handling with cross-process scenarios
- **Compliance Tests:** Automated checks against FSKit API contracts and macOS filesystem behavior
- **Performance Tests:** Criterion benchmarks for hot paths, memory profiling for new data structures
- **Regression Tests:** Full coverage of existing functionality to ensure no regressions

### Parallelization notes

- M-Review.1–3 can proceed in parallel (byte safety, handles, lazy I/O are largely independent)
- M-Review.4–6 can be implemented concurrently (xattrs, rename, PathConf are isolated changes)
- M-Review.7 requires careful integration testing (caller identity affects all operations)
- M-Review.8–10 depend on completing M-Review.1–7 (testing and documentation)

### References

- [Senior developer review](CodeReview/AgentFS-review-0.md)
- FSKit API documentation and Apple's FSKitSample reference
- Current implementation: `adapters/macos/xcode/AgentFSKitExtension/`
- Reviewer's concrete patches provided in review document
