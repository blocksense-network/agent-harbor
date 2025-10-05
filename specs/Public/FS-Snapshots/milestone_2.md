**Milestone 2: Mock Agent Integration Testing**
_Implementation:_ Create a realistic mock agent that simulates actual AI agent behavior without requiring real AI services:

- **Mock Agent Behavior:** The mock agent will perform realistic file operations (reading source files, creating output files, modifying existing files), include configurable work duration with sleep calls, and generate logs of its activities. It will simulate the patterns of real agents like Codex or Goose.
- **Docker Test Environment:** Build a minimal Alpine Linux Docker image containing the mock agent and Ruby runtime. This image will serve as a controlled environment for testing isolation and concurrency.
- **Parallel Execution Tests:** Write integration tests that launch multiple mock agents simultaneously in separate isolated workspaces. Verify that agents cannot see each other's changes and that all file modifications remain properly isolated.
- **Performance Testing:** Measure snapshot creation time, workspace cleanup time, and resource usage under concurrent load to ensure the system scales appropriately.

_Integration Testing:_ These tests will use real filesystem operations but controlled environments. Test on multiple filesystems (ext4, btrfs) in CI. Verify isolation guarantees and measure performance characteristics.
