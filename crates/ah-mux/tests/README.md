# Multiplexer Integration Tests

This directory contains integration tests that verify multiplexer implementations work correctly with real terminal multiplexers.

## Test Overview

The tests use a unified approach to verify all multiplexer backends with the same test logic, ensuring consistent behavior across different terminal multiplexers.

### Available Tests

#### 1. `test_all_multiplexer_basic_operations` (Ignored by default)
Tests basic multiplexer operations that should work across all implementations:
- Window creation and listing
- Pane splitting (where supported)
- Command execution
- Window/pane focusing
- Text sending (where supported)

**To run:**
```bash
cargo test --test integration_tests test_all_multiplexer_basic_operations
```

#### 2. `test_multiplexer_sizing_logic`
Verifies that border calculations and pane sizing math work correctly for different multiplexers.

**To run:**
```bash
cargo test --test integration_tests test_multiplexer_sizing_logic
```

#### 3. `test_advanced_pane_sizing_concept` (Ignored)
Conceptual demonstration of how complete pane sizing verification would work. Currently disabled as it requires additional infrastructure.

## Multiplexer-Specific Behavior

Each multiplexer has different capabilities and border characteristics:

| Multiplexer | Window Creation | Pane Splitting | Text Sending | Border Impact |
|-------------|----------------|----------------|--------------|----------------|
| tmux       | ✅             | ✅             | ✅          | 1 col vertical |
| kitty      | ✅             | ✅             | ❌          | None |
| wezterm    | ✅             | ✅             | ✅          | None |
| zellij     | ✅             | ❌ (layouts)   | ❌          | 2 cols, 1 row |
| screen     | ✅             | ✅             | ✅          | 1 col vertical |

## Running Tests

### Prerequisites

1. **Install terminal multiplexers** in your development environment:
   ```bash
   # macOS with Homebrew
   brew install tmux kitty wezterm zellij

   # Or use Nix (already configured in flake.nix)
   nix develop
   ```

2. **Ensure multiplexers are in PATH** and functional

### Test Execution

```bash
# Run all integration tests
cargo test --test integration_tests

# Run only the sizing logic test (doesn't require multiplexers)
cargo test --test integration_tests test_multiplexer_sizing_logic

# Run basic operations test (requires multiplexers)
cargo test --test integration_tests test_all_multiplexer_basic_operations -- --nocapture
```

### Test Output

The integration tests provide detailed output showing:
- Which multiplexers are detected and tested
- Success/failure of each operation
- Expected vs. actual behavior differences

## Future Enhancements

### Complete Pane Sizing Verification

To implement full pane sizing verification:

1. **Create measurement binary** (`measure_size.rs`) that outputs JSON with terminal dimensions
2. **Compile binary** as part of test setup
3. **Run in panes** using multiplexer-specific commands to execute and capture output
4. **Parse results** and verify sizing calculations

### Multiplexer-Specific Test Extensions

Add specialized tests for advanced features:
- tmux: Complex layouts, copy mode, session persistence
- wezterm: Workspace management, custom key bindings
- zellij: Layout files, plugin system
- kitty: Graphics protocol, remote control

## Troubleshooting

### Common Issues

1. **"No multiplexers available"**
   - Ensure multiplexers are installed and in PATH
   - Check that `which tmux`, `which kitty`, etc. work

2. **Test timeouts**
   - Some multiplexers need time to initialize
   - Increase sleep durations if needed

3. **Permission issues**
   - Some multiplexers require special permissions or configuration

### Debugging

Run tests with verbose output:
```bash
RUST_LOG=debug cargo test --test integration_tests -- --nocapture
```

## Test Architecture

The tests are designed to be:
- **Unified**: Same test logic runs against all multiplexers
- **Resilient**: Gracefully handle unsupported operations
- **Extensible**: Easy to add new multiplexers or test cases
- **Isolated**: Each multiplexer test runs independently
