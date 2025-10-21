# Rust Static Libraries for Swift FFI Integration

## Problem Statement

When building Rust crates for integration with Swift (or other languages) via FFI, we need to create static libraries (.a files) that can be linked into the Swift application. However, Cargo has limitations with `staticlib` crate types that depend on other Rust crates, leading to build errors.

## Root Cause

- Cargo allows `staticlib` crates to depend on regular Rust `lib` crates (producing `rlib`s)
- However, multiple `staticlib` crates with overlapping dependencies can cause duplicate symbols
- The recommended approach is a single "umbrella" FFI crate

## Recommended Solution: Single Umbrella FFI Crate

### Architecture

```
Workspace:
├── agentfs-core (rlib) - Core functionality
├── agentfs-proto (rlib) - Protocol types
├── agentfs-fskit-ffi (staticlib) - Umbrella crate for Swift
    └── Depends on: agentfs-core, agentfs-proto
    └── Exports: extern "C" functions for Swift
```

### Current Implementation Status

✅ **Working**: The `agentfs-ffi` crate can be built as a static library with dependencies on regular Rust crates. This was the correct approach all along.

### Implementation Details

The existing `agentfs-ffi` crate already follows the umbrella pattern:

1. **Cargo.toml Configuration**:

   ```toml
   [package]
   name = "agentfs-ffi"
   version = "0.1.0"

   [lib]
   crate-type = ["cdylib", "staticlib"]  # Both dynamic and static

   [dependencies]
   agentfs-core = { path = "../agentfs-core" }
   agentfs-proto = { path = "../agentfs-proto" }
   ```

2. **Build Process**:

   ```bash
   cargo build --package agentfs-ffi --release --target aarch64-apple-darwin
   # Produces: target/aarch64-apple-darwin/release/libagentfs_ffi.a
   ```

3. **Universal Binary Creation**:

   ```bash
   # Build for both architectures
   cargo build --package agentfs-ffi --release --target aarch64-apple-darwin
   cargo build --package agentfs-ffi --release --target x86_64-apple-darwin

   # Create universal binary
   lipo -create \
     target/aarch64-apple-darwin/release/libagentfs_ffi.a \
     target/x86_64-apple-darwin/release/libagentfs_ffi.a \
     -output libagentfs_ffi.a
   ```

### Swift Integration

1. **Generate C Headers**:
   - Use `cbindgen` to generate `agentfs_fskit_ffi.h`
   - Include in Swift project via bridging header

2. **Link Static Library**:
   - Add `.a` file to Xcode project
   - Set library search paths
   - Swift can call extern "C" functions directly

### Advantages

- ✅ No duplicate symbols
- ✅ Clean separation between Rust internals and FFI
- ✅ Single point of maintenance for Swift interface
- ✅ Follows established Rust FFI patterns

### Best Practices

- Keep FFI crate thin - just wrappers around core functionality
- Use opaque pointers (\*mut SomeType) for complex Rust types
- Handle memory management carefully (Box::into_raw, Box::from_raw)
- Generate bindings with tools like `uniffi` for better Swift ergonomics

## Alternative: Multiple Static Libraries

While technically possible, this approach is discouraged due to:

- Potential symbol duplication
- Complex linker configuration
- Increased maintenance burden

Only use if crates have completely disjoint dependencies.

## Tools and Dependencies

- `cbindgen`: Generate C headers from Rust
- `uniffi`: Generate idiomatic Swift bindings
- `cargo-xcframework`: Create XCFrameworks for multiple architectures

## References

- Rust FFI Book: <https://doc.rust-lang.org/nomicon/ffi.html>
- Cargo crate-types: <https://doc.rust-lang.org/cargo/reference/cargo-targets.html>
- Swift-Rust integration examples: rustls-ffi, Mozilla Application Services
