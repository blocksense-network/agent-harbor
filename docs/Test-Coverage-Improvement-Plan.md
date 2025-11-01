# Agent Harbor - Test Coverage Analysis & Improvement Plan

**Branch**: `testing/coverage-analysis-and-improvement-plan`
**Date**: 2025-11-01
**Author**: AI Agent Analysis for New Contributor Onboarding

## Executive Summary

This document provides a comprehensive analysis of test coverage across the Agent Harbor codebase and presents actionable improvement opportunities for new contributors following **Path D: Testing & Quality**.

### Current Test Coverage Overview

- **Total test functions**: ~1,421 across workspace
- **Rust unit tests**: 193 `#[test]` functions in ah-\* crates
- **Test modules**: 57 `#[cfg(test)]` modules
- **WebUI tests**: 25 TypeScript test files
- **AgentFS crates with tests**: 10 out of 12 crates
- **Integration test suites**: 7 major test directories
- **Testing frameworks in use**:
  - `insta` (snapshot testing) - 5 crates
  - `mockall` (mocking) - 1 crate
  - Playwright (E2E for WebUI/Electron)
  - Python pytest (integration tests)

### Key Findings

#### ✅ Well-Tested Areas

1. **AgentFS Core** (`agentfs-core`) - Comprehensive unit tests for all milestones
2. **CLI** (`ah-cli`) - 44 test functions, 12 test modules (~6,380 LOC)
3. **Multiplexer** (`ah-mux`) - 44 test functions, excellent integration test patterns
4. **TUI** (`ah-tui`) - 13 test functions, snapshot testing in place
5. **Sandbox enforcement** - Dedicated test suites for cgroups, overlays, networking, debugging

#### ⚠️ Moderate Coverage (Needs Improvement)

1. **Core** (`ah-core`) - ~28 unit/async tests for 3,029 LOC (still leaves large modules uncovered)
2. **REST Server** (`ah-rest-server`) - Only 1 placeholder test for 2,424 LOC
3. **WebUI** - 25 test files but focused on E2E over unit tests

#### ❌ Critical Gaps (Limited Behavioral Coverage)

1. **Sandbox crates** - Existing tests focus on config defaults; behavioral coverage is missing:
   - `sandbox-cgroups` - 5 unit tests (`crates/sandbox-cgroups/src/lib.rs:338`)
   - `sandbox-devices` - 6 unit tests (`crates/sandbox-devices/src/lib.rs:352`)
   - `sandbox-fs` - 5 unit tests (`crates/sandbox-fs/src/lib.rs:497`)
   - `sandbox-net` - 3 async tests (`crates/sandbox-net/src/lib.rs:205`)
   - `sandbox-proto` - 1 serialization test (`crates/sandbox-proto/src/lib.rs:48`)
   - `sandbox-seccomp` - 14 focused tests across path filtering and BPF generation (`crates/sandbox-seccomp/src/lib.rs`, `src/filter.rs`, `src/path_resolver.rs`, `src/notify.rs`)
     These suites rarely exercise real cgroup, device, or seccomp interactions.
     _Counts verified via_ `rg -g '*.rs' '#[test]' -c` _on 2025-11-01._
2. **TUI testing infrastructure** (`tui-testing`) - Integration and CLI smoke tests exist, but no failure-path or screenshot diff coverage (`crates/tui-testing/src/integration_tests.rs`).
3. **LLM API Proxy** (`llm-api-proxy`) - Six async tests cover error metrics only; no happy-path conversion assertions (`crates/llm-api-proxy/tests/basic_test.rs`).
4. **Several ah-\* crates** lack comprehensive coverage beyond narrow scenarios

## Good First Issues Tracking

### Where to Find Issues

The project does **not currently use GitHub Issues** for tracking good first issues. Instead:

1. **Check status files**: Look for `[ ]` unchecked items in `specs/Public/**/*.status.md`
2. **Search for TODOs**: 27 TODO/FIXME comments found in core crates
3. **Review this plan**: Detailed starter tasks listed below

### Recommended: Create GitHub Issue Labels

**Suggested labels to add**:

- `good-first-issue` - For beginner-friendly tasks
- `testing` - All test-related work
- `documentation` - Documentation improvements
- `beginner` - No deep domain knowledge required
- `intermediate` - Requires subsystem understanding
- `advanced` - Deep expertise needed

## Detailed Test Improvement Opportunities

### 🟢 Level 1: Beginner-Friendly (2-8 hours each)

#### 1.1. Add Test Logging Infrastructure

**Crates affected**: All test crates
**Current state**: Many tests print to stdout/stderr
**Goal**: Follow project guideline: "Each test MUST create a unique log file"

**Task breakdown**:

```rust
// Current pattern (non-compliant)
#[test]
fn test_something() {
    println!("Test output");  // ❌ Floods context
    assert!(result);
}

// Target pattern (compliant)
#[test]
fn test_something() {
    let log_path = create_unique_test_log("test_something");
    let mut log = File::create(&log_path).unwrap();

    writeln!(log, "Starting test...").unwrap();
    // Test logic with logging to file

    // On success: minimal stdout
    println!("✅ test_something passed");
    // On failure: print log path and size
}
```

**Where to start**:

1. Create helper in `crates/ah-test-utils/src/logging.rs` (new crate)
2. Add `create_unique_test_log(test_name: &str) -> PathBuf` helper
3. Update 5-10 tests in `ah-cli` as proof of concept
4. Document pattern in `AGENTS.md`

**Time estimate**: 4-6 hours
**Skills**: Rust basics, file I/O, testing patterns
**Value**: High - Improves AI agent context management

#### 1.2. Add Snapshot Tests for CLI Commands

**Crate**: `crates/ah-cli`
**Current state**: Parsing tests exist, but no output snapshot validation
**Goal**: Use `insta` to verify CLI output format stability

**Task breakdown**:

1. Review existing tests in `crates/ah-cli/src/agent/mod.rs`
2. Add snapshot tests for:
   - `ah agent fs snapshots <SESSION_ID>` output format
   - `ah agent fs branch list` output format
   - `ah task list` output format
   - Help text for major commands
3. Use `insta::assert_snapshot!()` for text output
4. Use `insta::assert_yaml_snapshot!()` for structured data

**Where to start**:

```rust
#[test]
fn test_snapshots_command_output() {
    let output = run_cli_command(&["agent", "fs", "snapshots", "test-session"]);
    insta::assert_snapshot!(output);
}
```

**Time estimate**: 4-8 hours
**Skills**: Rust, CLI design, snapshot testing
**Value**: Medium - Prevents CLI output regressions
**Reference**: See `crates/ah-mux/tests/` for excellent examples

#### 1.3. Expand TODOs into Test Cases

**Location**: Search for `TODO.*test` or `FIXME.*test` in code
**Current state**: 27 TODOs in core crates, many test-related
**Goal**: Convert TODOs into actual test functions

**Task breakdown**:

1. Run: `grep -rn "TODO\|FIXME" crates/ah-cli crates/ah-core crates/ah-tui --include="*.rs"`
2. For each TODO:
   - Create corresponding `#[test]` or `#[ignore]` test
   - Document why test is ignored if applicable
   - Link to tracking issue/status file
3. Remove TODO once test exists

**Where to start**: Pick simplest TODOs in `ah-cli`

**Time estimate**: 1-3 hours per TODO
**Skills**: Code reading, Rust
**Value**: Medium - Captures intent, prevents forgetting

#### 1.4. Add Unit Tests for Sandbox Crates

**Crates**: `sandbox-cgroups`, `sandbox-devices`, `sandbox-fs`, `sandbox-net`, `sandbox-seccomp`
**Current state**: Each crate already has a small set of configuration-default tests (5–14 total) but they never execute real cgroup, device, filesystem, or seccomp operations.
**Goal**: Strengthen behavioral coverage for core functions, including success, failure, and permission-denied paths.

**Critical functions to test**:

**`sandbox-cgroups`**:

- `create_cgroup()` - Test cgroup creation logic
- `set_memory_limit()` - Test memory limit parsing
- `set_cpu_limit()` - Test CPU limit calculations

**`sandbox-devices`**:

- `parse_device_whitelist()` - Test device string parsing
- `device_access_allowed()` - Test permission checks

**`sandbox-fs`**:

- `overlay_mount_options()` - Test mount option generation
- `create_overlay_dirs()` - Test directory structure creation

**`sandbox-net`**:

- `parse_network_rules()` - Test firewall rule parsing
- `generate_iptables_commands()` - Test command generation

**`sandbox-seccomp`**:

- `parse_syscall_filter()` - Test filter parsing
- `generate_seccomp_profile()` - Test BPF generation

**Where to start**: Pick one crate, extend the existing `#[cfg(test)]` module with behavior-driven tests that mock kernel interfaces (e.g., temp dirs + fake `/sys/fs/cgroup`) and validate parsing/translation helpers.

**Time estimate**: 6-10 hours per crate
**Skills**: Rust, Linux system programming basics
**Value**: **CRITICAL** - These are security-critical components!
**Reference**: Integration tests exist in `tests/cgroup-enforcement/`, build on patterns there

### 🟡 Level 2: Intermediate (8-16 hours each)

#### 2.1. Implement ah-core Test Suite

**Crate**: `crates/ah-core`
**Current state**: ~28 tests concentrated in `push.rs`, `editor.rs`, `devshell.rs`, and the `agent_tasks` integration suite, leaving most orchestrator modules unverified across 3,029 LOC.
**Goal**: Expand coverage to at least 50 focused tests spanning orchestration, state management, and failure handling.

**Modules needing tests**:

1. **`agent_executor.rs`** (0 tests currently)
   - Test `AgentExecutor::spawn()` with mock processes
   - Test working copy mode selection (snapshots vs. git)
   - Test environment variable injection
   - Test timeout handling

2. **`session.rs`** (0 tests currently)
   - Test session state transitions
   - Test session persistence (save/load)
   - Test session cleanup

3. **`task.rs`** (0 tests currently)
   - Test task state machine
   - Test task serialization
   - Test task validation

4. **`local_task_manager.rs`** (0 tests currently)
   - Test task launching with mock multiplexer
   - Test draft task creation/deletion
   - Test task listing and filtering

**Where to start**:

1. Add `#[cfg(test)]` module to each source file
2. Use `mockall` for external dependencies (filesystem, REST API)
3. Follow patterns from `ah-cli` tests

**Time estimate**: 12-16 hours
**Skills**: Rust, mocking, state machines
**Value**: **HIGH** - Core orchestration logic needs reliability
**Acceptance criteria**: Coverage >50 tests, all public functions tested

#### 2.2. Add REST Server Integration Tests

**Crate**: `crates/ah-rest-server`
**Current state**: Only 1 test function for 2,424 LOC
**Goal**: Full API endpoint coverage with integration tests

**Test categories**:

1. **Session endpoints**
   - `GET /api/v1/sessions` - List sessions
   - `POST /api/v1/sessions` - Create session
   - `GET /api/v1/sessions/{id}` - Get session details
   - `DELETE /api/v1/sessions/{id}` - Delete session

2. **Task endpoints**
   - `GET /api/v1/tasks` - List tasks
   - `POST /api/v1/tasks` - Create task
   - `GET /api/v1/tasks/{id}` - Get task details
   - `PUT /api/v1/tasks/{id}` - Update task

3. **Draft endpoints**
   - `GET /api/v1/drafts` - List drafts
   - `POST /api/v1/drafts` - Create draft
   - `DELETE /api/v1/drafts/{id}` - Delete draft

4. **SSE events**
   - Test event stream connection
   - Test event filtering
   - Test reconnection logic

**Where to start**:

1. Set up test harness with `actix-web::test`
2. Create mock database for test isolation
3. Test one endpoint category completely before moving to next

**Example test**:

```rust
#[actix_web::test]
async fn test_create_session() {
    let app = test::init_service(
        App::new().configure(configure_routes)
    ).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/sessions")
        .set_json(&CreateSessionRequest { ... })
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}
```

**Time estimate**: 12-16 hours
**Skills**: Rust, actix-web, REST APIs, async programming
**Value**: **HIGH** - REST API is critical interface
**Reference**: See REST API contract in `crates/ah-rest-api-contract`

#### 2.3. Add WebUI Component Unit Tests

**Directory**: `webui/app/src/components/`
**Current state**: 25 test files (mostly E2E), minimal unit test coverage
**Goal**: Unit test coverage for all UI components

**Components needing unit tests**:

1. **`TaskCard.tsx`**
   - Test rendering with different task states
   - Test action button interactions
   - Test timestamp formatting

2. **`DraftTaskCard.tsx`**
   - Test form validation
   - Test draft saving
   - Test draft deletion

3. **`SessionList.tsx`**
   - Test empty state
   - Test session filtering
   - Test session selection

4. **`AgentSelector.tsx`**
   - Test agent list rendering
   - Test agent selection
   - Test disabled state

**Where to start**:

1. Create `TaskCard.test.tsx` using Vitest + @solidjs/testing-library
2. Test rendering and prop variations
3. Test user interactions with `fireEvent`

**Example test**:

```typescript
import { render, screen } from '@solidjs/testing-library';
import { TaskCard } from './TaskCard';

describe('TaskCard', () => {
  it('renders task title and status', () => {
    const task = { id: '1', title: 'Test Task', status: 'running' };
    render(() => <TaskCard task={task} />);

    expect(screen.getByText('Test Task')).toBeInTheDocument();
    expect(screen.getByText('running')).toBeInTheDocument();
  });
});
```

**Time estimate**: 8-12 hours
**Skills**: TypeScript, SolidJS, Vitest, testing-library
**Value**: Medium - Prevents UI regressions
**Reference**: Existing E2E tests in `webui/e2e-tests/tests/`

#### 2.4. Implement LLM API Proxy Test Suite

**Crate**: `crates/llm-api-proxy`
**Current state**: Six async regression tests exercise error metrics only; no golden-path assertions despite complex conversion logic.
**Goal**: Full test coverage for API conversions, including successful Anthropic/OpenAI round-trips.

**Critical functions to test**:

1. **`converters/anthropic_to_openai.rs`**
   - Test message format conversion
   - Test streaming response transformation
   - Test error handling

2. **`converters/openai_to_anthropic.rs`**
   - Test request conversion
   - Test tool/function calling translation
   - Test parameter mapping

3. **`routing/mod.rs`**
   - Test provider selection logic
   - Test fallback routing
   - Test rate limiting

4. **`metrics/mod.rs`**
   - Test token counting
   - Test cost calculation
   - Test usage tracking

**Where to start**:

1. Create test fixtures with sample API requests/responses
2. Use `serde_json` for JSON assertion comparisons
3. Test both success and error paths

**Example test**:

```rust
#[test]
fn test_anthropic_to_openai_message_conversion() {
    let anthropic_msg = json!({
        "role": "user",
        "content": "Hello, Claude!"
    });

    let openai_msg = convert_message(&anthropic_msg).unwrap();

    assert_eq!(openai_msg["role"], "user");
    assert_eq!(openai_msg["content"], "Hello, Claude!");
}
```

**Time estimate**: 10-14 hours
**Skills**: Rust, API design, JSON processing
**Value**: **CRITICAL** - Conversion bugs cause agent failures
**Risk**: Breaking changes to external APIs

### 🔴 Level 3: Advanced (16-24 hours each)

#### 3.1. Property-Based Testing for AgentFS

**Crate**: `crates/agentfs-core`
**Current state**: Excellent unit tests, but no property-based tests
**Goal**: Use `proptest` or `quickcheck` to find edge cases

**Properties to test**:

1. **Snapshot immutability**:

   ```
   ∀ snapshot S, branch B from S:
     write(B, path, data) → read(S, path) unchanged
   ```

2. **Copy-on-write correctness**:

   ```
   ∀ file F, branches B1, B2 from same snapshot:
     write(B1, F, data1) ∧ write(B2, F, data2) →
     read(B1, F) = data1 ∧ read(B2, F) = data2
   ```

3. **Overlay semantics**:
   ```
   ∀ path P: ¬exists_upper(P) → read(P) = read_lower(P)
   ```

**Where to start**:

1. Add `proptest = "1.0"` to `Cargo.toml`
2. Create `crates/agentfs-core/tests/property_tests.rs`
3. Start with simple properties (e.g., write-read round-trip)

**Example**:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_write_read_roundtrip(
        path in "[a-z/]{1,50}",
        data in prop::collection::vec(any::<u8>(), 0..1024)
    ) {
        let fs = FsCore::new(FsConfig::default());
        fs.write(&path, &data).unwrap();
        let read_data = fs.read(&path).unwrap();
        assert_eq!(data, read_data);
    }
}
```

**Time estimate**: 16-24 hours
**Skills**: Advanced Rust, property-based testing, formal reasoning
**Value**: **VERY HIGH** - Finds subtle bugs in filesystem logic
**Reference**: AgentFS tests in `crates/agentfs-core/src/lib.rs`

#### 3.2. Mutation Testing for Critical Crates

**Crates**: `sandbox-seccomp`, `agentfs-core`, `ah-core`
**Current state**: No mutation testing
**Goal**: Use `cargo-mutants` to measure test quality

**What is mutation testing**:

- Automatically introduce bugs (mutants) into code
- Run test suite against each mutant
- If tests still pass, you've found a gap in coverage

**Where to start**:

1. Install: `cargo install cargo-mutants`
2. Run on one crate: `cargo mutants --package agentfs-core`
3. Review surviving mutants (bugs tests didn't catch)
4. Add tests to kill those mutants

**Example workflow**:

```bash
# Run mutation testing
cargo mutants --package sandbox-seccomp --output mutants.json

# Review results
cat mutants.json | jq '.survivors'

# Add tests for survivors
# Re-run until mutation score > 80%
```

**Time estimate**: 20-30 hours (across all crates)
**Skills**: Advanced testing, security mindset, debugging
**Value**: **CRITICAL** - Ensures security-critical code is well-tested
**Reference**: [Mutation testing guide](https://en.wikipedia.org/wiki/Mutation_testing)

#### 3.3. End-to-End Agent Workflow Tests

**Location**: `tests/scenarios/`
**Current state**: Basic scenario files exist, minimal automated tests
**Goal**: Full E2E tests covering real agent workflows

**Scenarios to test**:

1. **Simple task completion**
   - Start session
   - Create task
   - Agent executes task
   - Verify output
   - Clean up

2. **Multi-task session**
   - Create session with multiple tasks
   - Verify task ordering
   - Test task dependencies
   - Verify final state

3. **Snapshot and branch workflow**
   - Create session with snapshots enabled
   - Execute task
   - Verify snapshots created
   - Restore from snapshot
   - Verify state matches

4. **Error handling**
   - Test agent timeout
   - Test agent crash
   - Test invalid task input
   - Verify cleanup on error

**Where to start**:

1. Study `tests/tools/mock-agent/` for patterns
2. Create `tests/scenarios/e2e_workflow_test.py`
3. Use pytest fixtures for setup/teardown

**Example**:

```python
def test_simple_task_completion(test_db, mock_agent):
    # Create session
    session = create_session(agent="mock-agent")

    # Create task
    task = create_task(session.id, prompt="Write hello world")

    # Wait for completion
    result = wait_for_task_completion(task.id, timeout=60)

    # Verify output
    assert result.status == "completed"
    assert "hello world" in result.output
```

**Time estimate**: 24-32 hours
**Skills**: Python, pytest, system integration, AI agent workflows
**Value**: **VERY HIGH** - Tests the entire system end-to-end
**Reference**: See `tests/tools/mock-agent/tests/test_agent_integration.py`

## Recommended Starter Tasks

Based on your interests and skill level, here are the **top 5 recommended tasks** to start with:

### Option 1: Test Logging Infrastructure (Beginner, High Impact)

- **Task**: 1.1 - Add Test Logging Infrastructure
- **Time**: 4-6 hours
- **Impact**: Helps ALL future test development
- **Learning**: Rust I/O, testing patterns, project standards
- **Next steps**: Use the pattern in other test improvements

### Option 2: CLI Snapshot Tests (Beginner-Intermediate, Medium Impact)

- **Task**: 1.2 - Add Snapshot Tests for CLI Commands
- **Time**: 4-8 hours
- **Impact**: Prevents CLI regressions
- **Learning**: `insta` snapshot testing, CLI design
- **Next steps**: Expand to other CLI commands

### Option 3: Sandbox Unit Tests (Intermediate, Critical Impact)

- **Task**: 1.4 - Add Unit Tests for Sandbox Crates
- **Time**: 6-10 hours per crate
- **Impact**: **Security-critical** - highest value
- **Learning**: Linux sandboxing, security testing
- **Next steps**: Build on the existing config-focused tests (e.g., `sandbox-cgroups::tests`) by adding behavioral cases that simulate cgroup filesystems and verify enforcement logic. Start with `sandbox-cgroups`, then fan out to devices/fs/net/seccomp.

### Option 4: REST Server Integration Tests (Intermediate, High Impact)

- **Task**: 2.2 - Add REST Server Integration Tests
- **Time**: 12-16 hours
- **Impact**: Critical API reliability
- **Learning**: actix-web, API testing, async Rust
- **Next steps**: Expand to SSE event testing

### Option 5: WebUI Component Unit Tests (Intermediate, Medium Impact)

- **Task**: 2.3 - Add WebUI Component Unit Tests
- **Time**: 8-12 hours
- **Impact**: UI stability and confidence
- **Learning**: SolidJS, Vitest, component testing
- **Next steps**: Expand to more complex components

## Implementation Roadmap

### Phase 1: Foundation (Week 1)

1. ✅ Complete onboarding (read docs, setup environment)
2. Pick **one** starter task from recommendations above
3. Create feature branch: `testing/[task-name]`
4. Implement basic test infrastructure

### Phase 2: Execution (Week 2-3)

1. Implement chosen task completely
2. Follow all project guidelines (logging, comments, etc.)
3. Run full test suite: `just test-rust`
4. Submit PR with clear description

### Phase 3: Iteration (Week 4+)

1. Address PR feedback
2. Pick next task based on learnings
3. Consider advanced tasks (property-based, mutation testing)
4. Contribute to test documentation

## Testing Best Practices (Project-Specific)

### Must-Follow Guidelines

1. **Log files, not stdout** (AGENTS.md:54-56)

   ```rust
   // ❌ Don't do this
   println!("Test output");

   // ✅ Do this
   let log_path = create_unique_test_log("test_name");
   write_to_log(&log_path, "Test output");
   ```

2. **Automated, not interactive** (AGENTS.md:59)
   - Never require manual process management
   - All tests must run unattended in CI

3. **Timeout all tests** (AGENTS.md:14)

   ```rust
   #[test]
   #[timeout(Duration::from_secs(30))]
   fn test_might_hang() {
       // Test that could potentially hang
   }
   ```

4. **Never cheat assertions** (AGENTS.md:58)
   - Don't disable assertions to make tests pass
   - Fix the code or test, not the assertion

5. **Intention-focused comments**
   ```rust
   #[test]
   fn test_snapshot_immutability() {
       // Per AgentFS.md:12-14, snapshots must be immutable
       // See https://github.com/.../specs/Public/AgentFS/AgentFS.md
       let snapshot = create_snapshot();
       modify_branch(&snapshot);
       assert_eq!(read_snapshot(), original_state);
   }
   ```

### Testing Tools Already Available

- **Snapshot testing**: `insta` - Use for output verification
- **Mocking**: `mockall` - Use for external dependencies
- **Property testing**: Add `proptest` for advanced testing
- **Integration testing**: Python pytest for E2E scenarios
- **Browser testing**: Playwright for WebUI/Electron
- **Coverage**: Add `cargo-llvm-cov` for coverage reports

### Common Pitfalls to Avoid

1. **Don't skip test writing** - Tests are required, not optional
2. **Don't test implementation details** - Test public API behavior
3. **Don't create brittle tests** - Use helpers and fixtures
4. **Don't ignore flaky tests** - Fix or remove, don't ignore
5. **Don't batch unrelated tests** - One concern per test function

## Measuring Progress

### Key Metrics to Track

1. **Test count**: Target 50+ new tests per month
2. **Coverage**: Aim for >70% line coverage on new code
3. **Mutation score**: >80% for critical crates
4. **Test stability**: <1% flaky test rate
5. **Log compliance**: 100% of new tests use log files

### Tools for Measurement

```bash
# Count tests
cargo test --workspace --no-run 2>&1 | grep -o '"test"' | wc -l

# Run coverage (add to Cargo.toml first)
cargo install cargo-llvm-cov
cargo llvm-cov --html

# Run mutation testing
cargo install cargo-mutants
cargo mutants --package [crate-name]

# Check log file usage
grep -r "create_unique_test_log" crates/*/tests/ | wc -l
```

## Resources and References

### Project Documentation

- `AGENTS.md` - Testing guidelines (lines 52-59)
- `crates/ah-mux/tests/README.md` - Excellent test patterns
- `specs/Public/AgentFS/AgentFS-Core-Testing.md` - AgentFS testing strategy
- `specs/Public/TUI-Testing-Architecture.md` - TUI testing patterns

### External Resources

- [Rust Testing Guide](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [insta snapshot testing](https://insta.rs/)
- [proptest property testing](https://proptest-rs.github.io/proptest/)
- [cargo-mutants](https://mutants.rs/)
- [Testing Pyramid](https://martinfowler.com/articles/practical-test-pyramid.html)

### Community Help

- Check `specs/Public/*.status.md` for current work
- Study git history in `.agents/tasks/` for prompting patterns
- Review existing PRs for test patterns

## Next Steps

1. **Read this document thoroughly** (20-30 minutes)
2. **Pick ONE starter task** from recommendations
3. **Create feature branch**: `git checkout -b testing/[task-name]`
4. **Set up environment**: `just test-rust` to verify baseline
5. **Start coding**: Follow patterns from similar tests
6. **Submit PR**: Include this plan reference in description

## Conclusion

Agent Harbor has a solid testing foundation (1,421 tests) but significant gaps remain, especially in:

- Core orchestration logic (ah-core)
- Security-critical sandbox components
- REST API integration
- WebUI component unit tests
- Advanced testing (property-based, mutation)

The opportunities above provide clear, actionable paths for new contributors to make high-impact improvements while learning the codebase and testing best practices.

**Your contribution to testing will directly improve the reliability and security of AI-powered software development.** Let's build the future together! 🚀

---

_This plan was generated through comprehensive codebase analysis and follows the project's established guidelines in `AGENTS.md` and related documentation._
