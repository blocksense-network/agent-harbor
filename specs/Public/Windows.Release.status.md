# Windows Release Status (Future Milestone)

## Overview

- Goal: capture the preparatory work required to deliver a Windows build of Agent Harbor after the initial cross-platform (Linux/macOS) launch. The early focus is establishing a reproducible Windows development environment, followed by packaging/distribution readiness (including Electron app considerations) and automated + manual validation.
- Methodology: mirror the status file structure used elsewhere in `specs/Public/`, emphasizing concrete deliverables, automated verification, and companion manual testing utilities so engineers can iteratively validate Windows behavior.

### Outstanding Windows Tasks

- [ ] W1. Windows development environment bootstrap
- [ ] W2. Cross-compilation and toolchain feasibility study
- [ ] W3. Windows packaging (CLI, AgentFS, Electron)
- [ ] W4. End-to-end validation (automated + manual)

## W1. Windows development environment bootstrap

**Status:** Not started — we need a consistent way to provision Windows dev boxes/VMs with all prerequisites.

**Deliverables:**

- [ ] Define a Windows-specific dev environment script (PowerShell + winget/choco) that installs Rust toolchain, Node.js (for Electron), Git, WinFsp, and other dependencies described in `docs/Component-Architecture.md`.
- [ ] Provide a `docs/` guide and `just bootstrap-windows-dev` helper that configures environment variables, required PATH entries, and checks prerequisite versions.
- [ ] Create a manual checklist (`just manual-test-windows-env`) for engineers to confirm the environment (compilation of a sample crate, running mock TUI tools, validating WinFsp install).

**Verification (automated):**

- [ ] CI job (or scheduled workflow) running the bootstrap script inside a Windows runner to detect install regressions.
- [ ] Smoke test ensuring cargo builds `ah-cli` on Windows after bootstrap completes.

## W2. Cross-compilation and toolchain feasibility study

**Status:** Not started — assess whether we can produce Windows binaries from Linux/macOS hosts and identify gaps.

**Deliverables:**

- [ ] Document Rust/Cargo cross-compilation requirements (linkers, target triplets), including ARM vs x86 considerations.
- [ ] Prototype cross-building `ah-cli` and `ah-agent-start` crates from Linux using `cargo build --target x86_64-pc-windows-msvc` (or GNU) and capture blockers.
- [ ] Evaluate Electron packaging cross-platform limitations (especially around native dependencies and code signing).
- [ ] Produce a summary report with recommendations (continue cross-compilation or rely on native Windows builds).
- [ ] Manual experiment script (`just manual-test-windows-cross-build`) guiding developers through the cross-build attempt, highlighting expected outcomes/failures.

**Verification (automated):**

- [ ] CI experiment matrix job attempting cross-builds and reporting success/failure (non-blocking initially).
- [ ] Unit tests ensuring Windows-specific features (e.g., WinFsp integration) compile behind feature flags even when cross-building (using `--target` + conditional compilation).

## W3. Windows packaging (CLI, AgentFS, Electron)

**Status:** Not started — once environment and toolchain are viable, package deliverables for Windows users.

**Deliverables:**

- [ ] Define packaging strategy for CLI binaries (MSI/Zip) and AgentFS components (WinFsp hosts, services).
- [ ] Build/distribute Electron app with Windows-specific installers (handling code signing, auto-update prerequisites).
- [ ] Update `docs/Component-Architecture.md` with Windows packaging workflows and requirements (services, drivers, permissions).
- [ ] Create manual installer validation (`just manual-test-windows-packaging`) that guides QA through installation, verifying CLI commands, AgentFS mounting, and Electron launch.

**Verification (automated):**

- [ ] CI pipeline producing Windows artifacts and publishing them to staging storage.
- [ ] Automated smoke tests on Windows runners installing artifacts and launching key commands (`ah --help`, `ah tui`, AgentFS mount checks).

## W4. End-to-end validation (automated + manual)

**Status:** Not started — once packaging exists, validate the full Windows experience mirrors existing platforms.

**Deliverables:**

- [ ] Port core automated test suites to Windows (TUI scenarios, recorder integration, AgentFS tests with WinFsp).
- [ ] Ensure manual testing utilities described in earlier sections (multiplexer exerciser, command log viewer, docs site checks) have Windows-compatible variants or clear guidance when Windows support diverges.
- [ ] Update release documentation with Windows-specific troubleshooting, known limitations, and contribution guidelines.

**Verification (automated):**

- [ ] Windows CI matrix covering CLI/unit/integration suites and recording/TUI scenario tests.
- [ ] Manual smoke checklist (`just manual-test-windows-release`) combining environment, packaging, and workflow validation for human reviewers before shipping.
