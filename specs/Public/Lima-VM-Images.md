# Lima VM Setup — Linux Images for macOS Multi-OS Testing

## Summary

Define Lima VM image variants for agent-harbor multi-OS testing on macOS. All variants use Nix for agent-harbor components to ensure consistency across image types.

## VM Image Variants

### Alpine + Nix

- **Base**: Alpine Linux (minimal footprint)
- **Purpose**: Nix-first development environment
- **Package management**: Nix for all development tools and agent-harbor components
- **Target users**: Developers preferring declarative, reproducible environments

### Ubuntu LTS

- **Base**: Ubuntu 22.04/24.04 LTS
- **Purpose**: Maximum compatibility and familiar tooling
- **Package management**: APT for system packages, Nix for agent-harbor components, wide range of pre-installed package managers and language version managers for quick set up specific dependencies.
- **Target users**: General development teams wanting conventional Linux environment

## Common Requirements

All images include:

- **agent-harbor tooling**: Installed via Nix for version consistency
- **Filesystem snapshots**: ZFS or Btrfs support for Agent Time-Travel
- **Multi-OS integration**: SSH access via HTTP CONNECT; no dynamic VPNs required
- **Development essentials**: Git, build tools, terminal multiplexers

## Build Components

### Shared Infrastructure

- Common provisioning scripts (reused from Docker container setup)
- Nix flake for agent-harbor tools

Implementation Plan: See [Lima VM Images.status.md](Lima-VM-Images.status.md) for milestones, success criteria, and CI strategy.

### Deliverables

- Published QCOW2 images for Alpine+Nix and Ubuntu LTS variants
- Cloud-config and build scripts under `infra/lima/`
- Documentation snippet for `ah lima images fetch`
- CI job covering boot + agent enrollment regression
