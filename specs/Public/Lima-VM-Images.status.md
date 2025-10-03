### Overview

This document tracks the implementation status of the [Lima VM Setup — Linux Images for macOS Multi-OS Testing](Lima-VM-Images.md) functionality.

Goals:

- Deliver layered Lima VM images (Alpine+Nix and Ubuntu LTS variants) that provide a consistent multi-OS testing environment across macOS hosts
- Implement automated build pipelines, credential propagation, and comprehensive validation suites as specified in the design
- Provide a standard `ah lima images fetch` command and fleet planner integration
- Ship a robust, automated test matrix validating boot, agent enrollment, performance, and security

References:

- [Lima VM Images](Lima-VM-Images.md)
- [Multi-OS Testing](Multi-OS Testing.md)
- [REST Service](REST-Service/API.md) (for fleet planner integration)

### Component layout (parallel tracks)

- infra/lima/: Shared cloud-config templates, build scripts, and provisioning logic
- build/lima-images/: `just` recipes and CI automation for image creation and distribution
- ah-cli/lima-commands/: `ah lima images fetch` and fleet planner extensions
- test/lima-validation/: Smoke tests, performance benchmarks, and security validation suites

All components use standard shell scripting and Nix for reproducibility; macOS CI runners required for Lima testing.

### Deliverables

1. `agent-harbor-lima-alpine` and `agent-harbor-lima-ubuntu` images (GHCR)
   - Alpine+Nix and Ubuntu LTS base images with Nix, AgentFS dependencies, and AH tooling
   - Pre-configured cloud-init for SSH tunneling and agent auto-enrollment
   - Published with content digests and SHA256SUMS for verification

2. Build infrastructure
   - `just build-lima-images` recipe with parallel variant builds
   - Cloud-config templates under `infra/lima/`
   - CI pipeline for automated image building and publishing

3. AH CLI integration
   - `ah lima images fetch <variant>` with automatic verification and progress reporting
   - Fleet planner support for `profile = lima:<variant>` resolution
   - Sample `lima.yaml` configurations with shared directory mounts

4. Reference test suite
   - Automated smoke tests: boot → agent enrollment → multi-OS task execution
   - Performance benchmarks: filesystem snapshots, boot times, compression ratios
   - Security validation: port exposure, SSH hardening, access controls

### Success criteria (acceptance)

- Cross-platform: Alpine and Ubuntu variants boot successfully on macOS with Lima
- Automation: `just build-lima-images` produces verifiable QCOW2 artifacts for both variants
- Integration: `ah lima images fetch` downloads and verifies images; fleet planner resolves Lima profiles correctly
- Validation: CI pipeline runs comprehensive tests with clear pass/fail gates and regression detection
- Performance: Boot times and filesystem operations meet baseline expectations with automated benchmarking

### Milestones and tasks (with automated success criteria)

M0. Bootstrap and scaffolding (1–2d)

- Create image build structure (Nix flake + build scripts), GHCR workflows, and CI skeleton.
- Smoke test: build minimal Lima image; `limactl start` succeeds; basic SSH connectivity works.

M1. Base image authoring (3–5d)

- Fork/compose from `lima/alpine-lima` and `lima/ubuntu-lima` base repos with cloud-init inheritance.
- Create shared `cloud-config.yaml.erb` with Nix installation, AgentFS dependencies, and SSH configuration.
- Tests: cloud-init applies successfully; Nix installs and configures; SSH tunneling works.

M2. Build automation (4–6d)

- Author `justfile` recipe using `limactl vmtemplate` → `mkimage` pipeline.
- Implement `qemu-img convert -c` compression and XZ distribution with SHA256SUMS.
- Tests: artifacts build successfully; compression reduces size by 50%+; checksums verify integrity.

M3. AH CLI integration (3–5d)

- Implement `ah lima images fetch <variant>` with verification and progress reporting.
- Extend fleet planner to resolve `profile = lima:<variant>` to SSH endpoints and tag sets.
- Tests: CLI commands work end-to-end; fleet planner integration passes schema validation.

M4. Validation infrastructure (5–8d)

- Create GitHub Actions macOS runners workflow for boot → enroll → task execution smoke tests.
- Implement performance benchmarks (snapshot latency, disk throughput, boot times) with regression detection.
- Tests: security scanner confirms SSH-only exposure; host keys regenerate; password auth disabled.

Acceptance checklist (M0)

- [ ] CI builds succeed on macOS runners
- [ ] `limactl start` works with minimal image
- [ ] Basic SSH connectivity established

Acceptance checklist (M1)

- [ ] Alpine and Ubuntu base repos forked with cloud-init inheritance
- [ ] Shared cloud-config template generates valid YAML
- [ ] Nix installs and configures correctly in both variants

Acceptance checklist (M2)

- [ ] `just build-lima-images` produces QCOW2 artifacts
- [ ] Compression pipeline reduces image sizes effectively
- [ ] SHA256SUMS generated and verified correctly

Acceptance checklist (M3)

- [ ] `ah lima images fetch alpine` downloads and verifies images
- [ ] Fleet planner resolves `profile = lima:ubuntu` to correct endpoints
- [ ] Sample `lima.yaml` files enable workspace sharing

Acceptance checklist (M4)

- [ ] Automated smoke tests pass on both variants
- [ ] Performance benchmarks establish baselines with regression detection
- [ ] Security validation confirms proper hardening and SSH-only access

### Test strategy & tooling

- Unit Tests: Rust unit tests with `cargo test` for CLI components and configuration parsing; shell script unit tests for build recipes
- Integration Tests: VM boot and agent enrollment flows; image fetch and verification pipelines
- Performance Tests: Filesystem operations latency; boot time measurements; compression ratios
- Security Tests: Port scanning with `nmap`; SSH configuration validation; access control verification
- End-to-End Tests: Multi-OS task execution across Lima variants; fleet planner resolution
- Tools: `cargo test` for Rust components, pytest for test harnesses; GitHub Actions macOS runners; custom shell scripts for VM operations

### Risks & mitigations

- Lima/QEMU version compatibility: Pin versions in flake.nix; test against multiple macOS versions
- macOS CI resource constraints: Use efficient image compression; parallelize tests where possible
- Network reliability for image downloads: Implement retry logic; support offline/cached workflows
- Security hardening complexity: Start with minimal attack surface; iterate based on security audits

### Parallelization notes

- Infrastructure Track (M0-M2) can proceed independently of Tooling Integration (M3)
- Image authoring (M1.1-M1.3) can be parallelized across Alpine and Ubuntu variants
- Validation Track (M4) can start after basic infrastructure (M0-M1) is stable
- Build automation (M2) and tooling integration (M3) can proceed in parallel once base images exist

### Status tracking

- M0: pending
- M1: pending
- M2: pending
- M3: pending
- M4: pending

### Change log

- Initial plan created; aligned with devcontainer design patterns for comprehensive milestones, acceptance criteria, and test strategies
