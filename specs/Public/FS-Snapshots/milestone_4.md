**Milestone 4: SSH/Remote Execution Framework**
_Implementation:_ Build remote execution capabilities using Docker containers with SSH servers for realistic testing:

- **SSH Test Infrastructure:** Extend the test-purpose docker image with a configured SSH server. Use this for testing remote execution without requiring actual remote machines.
- **File Synchronization:** Implement both one-shot sync (using `rsync`) and persistent sync (using Mutagen) approaches. Handle edge cases like network interruptions and large file transfers.
- **Remote Filesystem Detection:** Extend the filesystem detection logic to work over SSH connections. Cache detection results to avoid repeated SSH calls.
- **Error Handling:** Comprehensive error handling for network failures, authentication issues, insufficient remote permissions, and missing remote tools.

_Integration Testing:_ Use Docker containers as SSH targets to test the complete remote workflow. Launch a container, establish SSH connection, sync files, create snapshots remotely, execute agents, and retrieve results. Test concurrent remote executions and verify isolation.
