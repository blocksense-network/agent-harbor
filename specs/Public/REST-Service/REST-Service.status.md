# Agent Harbor REST Service - Implementation Status

## Overview

This document tracks the implementation progress of the Agent Harbor REST service as specified in [API.md](API.md) and [Tech-Stack.md](Tech-Stack.md). The REST service provides the network API for creating and managing agent coding sessions, supporting both on-premises deployments and local developer workflows.

## Architecture Summary

The REST service implementation spans multiple Rust crates as defined in [Repository-Layout.md](../Repository-Layout.md):

- **ah-core**: Core domain types including TaskManager trait
- **ah-rest-api-contract**: Shared types, schemas, validation (contract-first design)
- **ah-rest-client**: Type-safe production client library implementing TaskManager trait
- **ah-rest-mock-client**: Mock client with simulated tokio time for MVVM-style testing
- **ah-rest-server**: Production server with SQLite backend and task orchestration
- **Server dependency injection**: `crates/ah-rest-server/src/dependencies.rs` wires SQLite-backed services while `crates/ah-rest-server/src/mock_dependencies.rs` reuses the TUI mock TaskManager inside the HTTP handlers.
- **Rust mock server binary**: `crates/ah-rest-server/src/bin/mock_server.rs` launches the dependency-injected mock backend for `just manual-test-tui-remote-rust-mock`, while the TypeScript server remains available via `just manual-test-tui-remote-typescript-mock`.
- **ah-cli integration**: `ah webui` and `ah agent access-point` commands

**Note**: The mock REST server already exists in `webui/mock-server/` and will be used for testing the production client.

## Implementation Strategy

The implementation follows a **bottom-up, contract-first** approach:

1. **Milestone 1 & 2**: Define shared API contract types and TaskManager trait in ah-core
2. **Milestone 3**: Implement mock client with tokio time simulation for TUI testing
3. **Milestone 4**: Build production REST client library tested against existing webui/mock-server
4. **Milestones 5-13**: Implement production server with task orchestration incrementally
5. **Milestones 14-16**: Integrate with CLI commands (`ah webui`, `ah agent access-point`, `ah agent enroll`)
6. **Milestone 17**: Implement WebUI proxy integration
7. **Milestone 18**: Comprehensive end-to-end integration testing

This strategy enables parallel development tracks while maintaining type safety and API consistency. The mock client (M3) enables TUI development to continue independently while the production client (M4) validates the existing mock server.

---

## Milestone 1: API Contract Foundation

**Status**: Completed

### Deliverables

- [x] Create `crates/ah-rest-api-contract` with shared request/response types
- [x] Implement request validation for all POST/PUT endpoints
- [x] Define SSE event types matching [API.md](API.md) specification
- [x] Add OpenAPI schema generation with utoipa annotations
- [x] Implement JSON serialization/deserialization for all types
- [x] Define error types following Problem+JSON format (RFC 7807)
- [x] Add pagination, filtering, and query parameter types
- [x] Implement idempotency key handling types

### Verification

- [x] All API types serialize/deserialize correctly with serde_json
- [x] Request validation rejects invalid inputs with descriptive errors
- [x] OpenAPI schema generated at `/api/v1/openapi.json` validates with openapi-validator
- [x] Problem+JSON error responses include correct status codes and details
- [x] Pagination types correctly handle edge cases (empty results, invalid page numbers)
- [x] SSE event types match [API.md](API.md) event taxonomy exactly
- [x] All required fields validated, optional fields handled correctly
- [x] Idempotency keys properly typed and validated (ULID format)

### Implementation Details

The API contract foundation has been implemented in the `crates/ah-rest-api-contract` crate with the following key components:

- **Core Types**: Complete type system for REST API requests/responses including `CreateTaskRequest`, `Session`, `SessionEvent`, and supporting types
- **Event System**: Extended SSE event types to include agent activity events (`Thought`, `ToolUse`, `ToolResult`, `FileEdit`) with proper serialization
- **Validation**: Comprehensive validation logic for all POST/PUT endpoints using the `validator` crate
- **Error Handling**: Problem+JSON format error responses with RFC 7807 compliance
- **OpenAPI Generation**: Full utoipa integration for automatic OpenAPI schema generation
- **Idempotency**: ULID-based idempotency key validation and handling
- **Pagination**: Query parameter types for pagination, filtering, and sorting

### Key Source Files

- `crates/ah-rest-api-contract/src/types.rs` - Core API type definitions
- `crates/ah-rest-api-contract/src/error.rs` - Error types and Problem+JSON responses
- `crates/ah-rest-api-contract/src/validation.rs` - Validation logic and comprehensive tests
- `crates/ah-rest-api-contract/src/lib.rs` - OpenAPI schema generation

### Test Strategy

- Unit tests for each type's validation logic
- Property-based tests for serialization round-trips (using proptest)
- Schema validation tests ensuring OpenAPI spec matches implementation
- Edge case tests for boundary values (empty strings, max lengths, special characters)

---

## Milestone 2: TaskManager Trait Definition in ah-core

**Status**: Completed

**Reference Implementation**: See `crates/ah-core/src/task_manager.rs` for the trait design and `MockTaskManager` implementation that demonstrates the intended interface.

### Deliverables

- [x] Move/define `TaskManager` trait in `crates/ah-core` (based on PoC design)
- [x] Define domain types: `TaskLaunchParams`, `TaskLaunchResult`, `SaveDraftResult`
- [x] Define event types: `TaskEvent`, `TaskExecutionStatus`, `LogLevel`, `ToolStatus`
- [x] Define data types: `Repository`, `Branch`, `TaskInfo`, `SelectedModel` (reused from ah-domain-types)
- [x] Document trait contract and implementation requirements
- [x] Ensure trait is async-trait compatible
- [x] Ensure trait supports both local and remote implementations
- [x] Make trait Send + Sync for concurrent use
- [x] Consider extracting to `ah-domain-types` crate if ah-core becomes too large (decided to keep in ah-core for now)

### Verification

- [x] TaskManager trait compiles and is well-documented
- [x] All domain types have proper derives (Debug, Clone, Serialize, Deserialize)
- [x] Event types match SSE event structure from [API.md](API.md)
- [x] Trait methods have clear contracts in documentation
- [x] Mock client (Milestone 3) can implement trait
- [x] Production client (Milestone 4) can implement trait
- [x] Local mode implementation (future) can implement trait
- [x] Types are Send + Sync for use in async contexts
- [x] No unnecessary dependencies in ah-core (keep it lean)
- [x] Trait design is compatible with PoC TUI MVVM patterns

### Implementation Details

The TaskManager trait has been implemented in `crates/ah-core/src/task_manager.rs` with the following key components:

- **TaskManager Trait**: Abstract trait defining the interface for task launching across different execution modes (local, remote, mock)
- **Domain Types**: `TaskLaunchParams` for launch requests, `TaskLaunchResult` for responses, `SaveDraftResult` for draft operations
- **Event System**: `TaskEvent` enum with variants for status changes, logs, thoughts, tool usage, and file edits, matching SSE event structure
- **Status Types**: `TaskExecutionStatus` for execution states (queued, running, completed, etc.), distinct from internal `TaskStatus`
- **Dependencies**: Added `ah-domain-types`, `async-trait`, and `futures` to Cargo.toml
- **Naming Resolution**: Renamed existing concrete `TaskManager` to `TaskStateManager` to avoid conflicts
- **Time Handling**: Used `chrono::DateTime<Utc>` for timestamps to align with REST API contract

### Key Source Files

- `crates/ah-core/src/task_manager.rs` - TaskManager trait and all domain types
- `crates/ah-core/src/lib.rs` - Updated exports for the new trait and types
- `crates/ah-core/Cargo.toml` - Added required dependencies

### Test Strategy

- Compile tests ensuring trait can be implemented (cargo check passes)
- All existing ah-core tests continue to pass
- Type constraint verification through compilation
- Documentation provides clear implementation guidance

---

## Milestone 3: Mock REST Client

**Status**: Completed

**Reference Implementation**: See `crates/ah-core/src/task_manager.rs` for `MockTaskManager` implementation demonstrating the interface, and MVVM testing patterns in the ah-tui crate.

### Deliverables

- [x] Create `crates/ah-rest-mock-client` implementing TaskManager trait from ah-core
- [x] In-memory state management for tasks, sessions, drafts, repositories
- [x] Mock implementations for all TaskManager trait methods
- [x] Simulated async operations compatible with tokio::time::pause()
- [x] Configurable delays for testing async behavior
- [x] Realistic event stream generation matching SSE event types
- [x] Support for failure injection and edge case simulation
- [x] Deterministic task ID generation for reproducible tests
- [x] Thread-safe concurrent operation support

### Verification

- [x] Mock client implements TaskManager trait correctly
- [x] launch_task() validates inputs and returns TaskLaunchResult
- [x] task_events_stream() generates realistic event sequences
- [x] get_initial_tasks() returns drafts and tasks with proper structure
- [x] save_draft_task() simulates persistence with configurable delay
- [x] list_repositories() returns mock repository data
- [x] list_branches() returns repository-specific branches
- [x] Works correctly with tokio::time::pause() for accelerated testing
- [x] Configurable delays allow testing race conditions
- [x] Failure injection enables error path testing
- [x] Thread-safe for concurrent operation from multiple tasks
- [x] Deterministic behavior enables reproducible test scenarios
- [x] Compatible with existing TUI MVVM test patterns from PoC
- [x] ViewModels can use mock client with time-based test scenarios

### Implementation Details

The MockRestClient has been implemented in `crates/ah-rest-mock-client/src/lib.rs` with the following key components:

- **MockRestClient Struct**: Thread-safe in-memory storage for tasks, drafts, and repositories using Arc<RwLock>
- **TaskManager Implementation**: Complete implementation of all trait methods with realistic behavior
- **Event Stream Generation**: Sophisticated event stream simulation with 50+ realistic events including status changes, logs, thoughts, tool usage, and file edits
- **Configurable Behavior**: Support for custom delays and failure injection for testing edge cases
- **Deterministic IDs**: Hash-based task ID generation for reproducible test scenarios
- **Thread Safety**: All operations are async and thread-safe using Tokio primitives
- **PoC Compatibility**: Event sequences match the patterns from previous PoC implementations for seamless integration

### Key Source Files

- `crates/ah-rest-mock-client/src/lib.rs` - Complete MockRestClient implementation
- `crates/ah-rest-mock-client/Cargo.toml` - Dependencies and crate configuration

### Test Strategy

- Unit tests for each TaskManager method (8 comprehensive tests covering all functionality)
- Input validation tests for launch_task parameters
- Failure injection tests for error path simulation
- Deterministic ID generation verification
- Repository and branch listing validation
- Draft task persistence simulation
- All tests pass with 100% success rate

---

## Milestone 4: REST Client Library

**Status**: Completed

### Deliverables

- [x] Create `crates/ah-rest-client` wrapping reqwest with type-safe API
- [x] Implement TaskManager trait from ah-core for production use
- [x] Implement all API methods from [API.md](API.md)
- [x] SSE event stream consumption with reconnection logic
- [x] Connection pooling and HTTP/2 support
- [x] Retry logic with exponential backoff
- [x] Authentication header injection (API Key, JWT)
- [x] Error handling with rich error types
- [x] Timeout configuration per endpoint
- [x] Request/response logging integration with tracing
- [x] Optional TLS certificate validation configuration
- [x] Improvements to webui/mock-server as identified during testing

### Verification

- [x] Client successfully creates tasks via POST /api/v1/tasks against webui/mock-server
- [x] Client lists sessions with filtering and pagination against webui/mock-server
- [x] Client streams SSE events with automatic reconnection on disconnect
- [x] Client handles 429 rate limit responses with Retry-After
- [x] Client retries failed requests with exponential backoff
- [x] Client correctly sets Authorization headers (ApiKey and Bearer)
- [x] Client parses Problem+JSON errors into rich error types
- [x] Client respects per-endpoint timeouts
- [x] Client logs requests/responses at appropriate trace levels
- [x] Client works with self-signed certificates when validation disabled
- [x] Client connection pool reuses connections efficiently
- [x] Client handles server-side errors (4xx, 5xx) gracefully
- [x] All integration tests pass against existing webui/mock-server
- [x] Identified mock-server improvements documented and implemented
- [x] TaskManager trait implementation matches mock client behavior (Milestone 3)
- [x] Regression tests in `crates/ah-rest-server/tests/mock_server.rs` spin up the Rust mock server via dependency injection and drive it through `ah-rest-client` + `GenericRestTaskManager`.

### Implementation Details

The RestTaskManager has been implemented as a wrapper around the existing RestClient in `crates/ah-rest-client/src/task_manager.rs` with the following key components:

- **RestTaskManager Struct**: Wrapper around Arc<RestClient> implementing TaskManager trait for production use
- **TaskManager Implementation**: Complete implementation translating between TaskManager interface and REST API calls
- **Event Stream Handling**: Async stream wrapper around SessionEventStream with proper error handling and event conversion
- **Request Translation**: Conversion between TaskLaunchParams and CreateTaskRequest API structures
- **Response Translation**: Conversion between API responses and TaskInfo/TaskEvent structures
- **Error Handling**: Rich error conversion from RestClientError to TaskLaunchResult and TaskEvent error streams
- **Authentication**: Leverages existing AuthConfig for API key and bearer token authentication
- **Connection Management**: Uses reqwest's built-in connection pooling and HTTP/2 support
- **Timeout Handling**: Inherits timeout configuration from underlying RestClient
- **Logging Integration**: Uses tracing for request/response logging through the existing client

### Key Source Files

- `crates/ah-rest-client/src/task_manager.rs` - RestTaskManager implementation with TaskManager trait
- `crates/ah-rest-client/src/client.rs` - Enhanced RestClient with HTTP client functionality
- `crates/ah-rest-client/src/auth.rs` - Authentication configuration and header injection
- `crates/ah-rest-client/src/error.rs` - Rich error types and Problem+JSON handling
- `crates/ah-rest-client/src/sse.rs` - SSE event stream handling with reconnection logic

### Test Strategy

- Unit tests for TaskManager trait implementation (3 comprehensive tests)
- Integration tests against webui/mock-server (6 comprehensive tests covering all TaskManager methods)
- Shared mock server management ensuring single server instance across all tests
- Individual test isolation - each test works independently or as part of test suite
- Input validation tests for task launch parameters
- Error handling tests for API failures
- Authentication configuration tests
- SSE event parsing tests
- All tests pass with 100% success rate
- TaskManager interface compatibility verified

---

## Milestone 5: Production Server - Core Infrastructure

**Status**: Completed

### Deliverables

- [x] Create `crates/ah-rest-server` with Axum HTTP server
- [x] SQLite database backend using sqlx with migrations
- [x] Database schema for tasks, sessions, events, workspaces
- [x] Server initialization and graceful shutdown
- [x] Configuration loading (bind address, port, database path)
- [x] Health check endpoint implementation
- [x] CORS middleware from tower-http
- [x] Request tracing middleware with request IDs
- [x] OpenAPI documentation serving at /api/v1/openapi.json
- [x] Swagger UI or RapiDoc serving at /api/docs
- [x] Rate limiting middleware (tower-governor or Tower's RateLimitLayer)

### Verification

- [x] Server starts on configured bind address and port
- [x] Health check endpoint returns 200 OK
- [x] SQLite database created with correct schema
- [x] Database migrations run successfully on startup
- [x] Server logs requests with unique request IDs
- [x] CORS headers present for OPTIONS requests
- [x] OpenAPI schema accessible at /api/v1/openapi.json
- [x] Swagger UI accessible at /api/docs
- [x] Rate limiting returns 429 with Retry-After header
- [x] Server shuts down gracefully on SIGTERM/SIGINT
- [x] Server handles concurrent requests correctly

### Implementation Details

The production server core infrastructure has been implemented in `crates/ah-rest-server` with the following key components:

- **Axum HTTP Server**: Full-featured web framework with async support and Tower ecosystem integration
- **SQLite Backend**: Uses `ah-local-db` crate for database operations with connection pooling
- **Middleware Stack**: Comprehensive middleware including request tracing, compression, CORS, and rate limiting
- **Configuration System**: Flexible configuration loading with environment variable support
- **Health Endpoints**: Standard health check, readiness check, and version endpoints
- **OpenAPI Integration**: Auto-generated OpenAPI spec with Swagger UI interface
- **Error Handling**: Problem+JSON error responses following RFC 7807
- **Request Tracing**: Unique request IDs and structured logging with tracing

### Key Source Files

- `crates/ah-rest-server/src/server.rs` - Main server implementation with routing and middleware
- `crates/ah-rest-server/src/config.rs` - Configuration management
- `crates/ah-rest-server/src/state.rs` - Shared application state
- `crates/ah-rest-server/src/handlers/health.rs` - Health check endpoints with OpenAPI annotations
- `crates/ah-rest-server/src/handlers/openapi.rs` - OpenAPI spec generation and Swagger UI
- `crates/ah-rest-server/src/middleware.rs` - Custom middleware including rate limiting

### Test Strategy

- Integration tests using reqwest against running server
- Database migration tests ensuring idempotency
- Concurrent request tests validating thread safety
- Graceful shutdown tests ensuring in-flight requests complete
- Rate limiting tests ensuring limits enforced correctly

---

## Milestone 6: Production Server - Task Lifecycle

**Status**: Completed

### Deliverables

- [x] Implement POST /api/v1/tasks endpoint
- [x] Task state machine (queued → provisioning → running → completed/failed)
- [x] Integration with `ah agent start` for task execution
- [x] Integration with `ah agent record` for session recording
- [x] Task process lifecycle management (spawn, monitor, cleanup)
- [x] Task output capture and storage
- [x] Session state persistence in SQLite
- [x] Task cleanup on server shutdown
- [x] Resource limits enforcement (max concurrent tasks)
- [x] Workspace provisioning and cleanup

### Verification

- [x] POST /api/v1/tasks creates task record in database
- [x] Task transitions through lifecycle states correctly
- [x] Server spawns `ah agent record` wrapping `ah agent start`
- [x] Task output captured and stored in database
- [x] GET /api/v1/sessions/{id} returns correct task state
- [x] Task cleanup removes workspace and process on completion
- [x] Server respects max-concurrent-tasks limit
- [x] Failed tasks transition to failed state with error message
- [x] Cancelled tasks can be stopped via DELETE /api/v1/sessions/{id}
- [x] Server recovers running tasks after restart (or marks them as failed)

### Implementation Details

The production server task lifecycle has been fully implemented with comprehensive database integration, snapshot caching, and agent orchestration:

- **Database Session Store**: Complete SQLite persistence using `DatabaseSessionStore` with foreign key constraints and proper migration support
- **Task Executor Service**: Background service managing task state transitions, process lifecycle, and resource limits
- **Snapshot Caching**: Global LRU cache for repository snapshots with disk capacity limits and eviction policies
- **Workspace Provisioning**: Intelligent provisioning logic using cached snapshots or fresh checkout with global mutex locking
- **Agent Integration**: Direct integration with `ah agent record` wrapping `ah agent start`, supporting configuration forwarding and snapshot restoration
- **State Machine**: Tasks progress through queued → provisioning → running → completed/failed states with proper error handling
- **Resource Limits**: Configurable max concurrent tasks with enforcement and queue management
- **Task Cancellation**: DELETE endpoint for graceful task termination and cleanup
- **Database Migrations**: Versioned schema migrations including drafts table, task metadata, and repository tracking
- **Local Task Manager**: Complete implementation with database-backed draft storage and repository/branch enumeration
- **Configuration System**: Layered configuration support with `--config` parameter forwarding to agent processes
- **Dependency injection hooks**: `state.rs` now stores trait objects supplied by `dependencies.rs`, enabling alternative backends such as `mock_dependencies.rs` which reuses `ah-rest-mock-client` behind the HTTP surface.
- **Rust mock server binary**: `src/bin/mock_server.rs` exposes the injected mock backend for local workflows; `just manual-test-tui-remote-rust-mock` boots this binary while `just manual-test-tui-remote-typescript-mock` continues to exercise the Express-based server.

### Key Source Files

- `crates/ah-rest-server/src/executor.rs` - TaskExecutor with snapshot caching, workspace provisioning, and agent process spawning
- `crates/ah-rest-server/src/models.rs` - DatabaseSessionStore with SQLite persistence and API contract mapping
- `crates/ah-rest-server/src/state.rs` - AppState integration and configuration parameter passing
- `crates/ah-rest-server/src/main.rs` - Server CLI with --config parameter support
- `crates/ah-core/src/agent_executor.rs` - Core agent spawning logic shared between server and local modes
- `crates/ah-core/src/task_manager.rs` - LocalTaskManager with database-backed draft storage and repository management
- `crates/ah-core/src/db.rs` - DatabaseManager with repository and draft CRUD operations
- `crates/ah-local-db/src/models.rs` - Database models including DraftRecord, DraftStore, and enhanced TaskRecord
- `crates/ah-local-db/src/migrations.rs` - Database migrations including drafts table and schema versioning
- `crates/config-core/src/lib.rs` - Configuration layering system with CLI --config support

### Test Strategy

- End-to-end integration tests creating tasks via REST API and monitoring lifecycle completion
- Process lifecycle tests (spawn, monitor, kill, cleanup) with resource limit enforcement
- Database persistence tests ensuring session state survives server restarts
- Snapshot caching tests with LRU eviction and workspace provisioning logic
- Configuration forwarding tests verifying --config parameters reach agent processes
- Local Task Manager tests with database-backed draft storage and repository enumeration
- Concurrent task execution tests respecting max-concurrent-tasks limits
- Agent integration tests validating `ah agent record` and `ah agent start` command construction

---

## Milestone 7: Production Server - Event Streaming

**Status**: Not Started

### Deliverables

- [ ] Implement GET /api/v1/sessions/{id}/events (SSE endpoint)
- [ ] Event broadcasting system for task updates
- [ ] Live event capture from `ah agent record` output
- [ ] Event storage in SQLite for replay
- [ ] Event filtering by type, level, time range
- [ ] Paginated historical event API
- [ ] Keep-alive mechanism for SSE connections
- [ ] Connection management (tracking active SSE clients)
- [ ] Backpressure handling for slow clients

### Verification

- [ ] SSE endpoint streams events as task executes
- [ ] Events persist to database during task execution
- [ ] Historical events retrievable via pagination
- [ ] Event filtering works correctly (type, level, time range)
- [ ] SSE clients receive keep-alive messages
- [ ] SSE connections close cleanly on task completion
- [ ] Multiple clients can stream same session simultaneously
- [ ] Slow clients don't block task execution
- [ ] Server handles SSE client disconnection gracefully

### Test Strategy

- SSE consumption tests validating event format and order
- Multi-client tests ensuring concurrent streams work
- Historical event query tests with various filters
- Backpressure tests with deliberately slow clients
- Connection lifecycle tests (connect, disconnect, reconnect)

---

## Milestone 8: Production Server - Session Management

**Status**: Not Started

### Deliverables

- [ ] Implement GET /api/v1/sessions with filtering and pagination
- [ ] Implement GET /api/v1/sessions/{id}
- [ ] Implement POST /api/v1/sessions/{id}/pause
- [ ] Implement POST /api/v1/sessions/{id}/resume
- [ ] Implement POST /api/v1/sessions/{id}/stop
- [ ] Implement DELETE /api/v1/sessions/{id}
- [ ] Session state transitions (running ↔ paused, running → stopped)
- [ ] Process signal handling (SIGSTOP/SIGCONT for pause/resume)
- [ ] Session cleanup and archival

### Verification

- [ ] GET /api/v1/sessions returns sessions with correct pagination
- [ ] Filtering by status, repository, projectId works correctly
- [ ] Session pause sends SIGSTOP and updates state to paused
- [ ] Session resume sends SIGCONT and updates state to running
- [ ] Session stop sends SIGTERM and waits for graceful shutdown
- [ ] Session delete sends SIGKILL and performs immediate cleanup
- [ ] State transitions validated (can't resume stopped session, etc.)
- [ ] Session metadata includes recent_events for active sessions
- [ ] Paused sessions don't consume CPU resources

### Test Strategy

- Integration tests for each session lifecycle operation
- State machine tests validating valid/invalid transitions
- Process signal tests ensuring pause/resume actually work
- Cleanup tests ensuring resources freed on stop/delete
- Pagination tests with large numbers of sessions

---

## Milestone 9: Production Server - File Operations

**Status**: Not Started

### Deliverables

- [ ] Implement GET /api/v1/sessions/{id}/files
- [ ] Implement GET /api/v1/sessions/{id}/files/{filePath}
- [ ] Implement GET /api/v1/sessions/{id}/diff/{filePath}
- [ ] Implement GET /api/v1/sessions/{id}/diff (multi-file)
- [ ] Implement GET /api/v1/sessions/{id}/workspace/files
- [ ] File change tracking during task execution
- [ ] Diff generation with configurable context lines
- [ ] Support for unified, split, and HTML diff formats

### Verification

- [ ] Files endpoint lists all modified files during session
- [ ] File metadata includes lines added/removed and timestamps
- [ ] Individual file diffs generated correctly
- [ ] Multi-file diffs aggregate changes correctly
- [ ] Workspace file browsing works recursively
- [ ] Diff context lines configurable (3, 5, 10)
- [ ] HTML diff format generates valid HTML
- [ ] Binary files handled gracefully (no diff, just metadata)

### Test Strategy

- Integration tests creating sessions that modify files
- Diff generation tests comparing against git diff output
- Edge case tests (empty files, binary files, renames, large files)
- Format conversion tests (unified → split → HTML)

---

## Milestone 10: Production Server - Chat and Context

**Status**: Not Started

### Deliverables

- [ ] Implement GET /api/v1/sessions/{id}/chat
- [ ] Implement POST /api/v1/sessions/{id}/chat/messages
- [ ] Implement GET /api/v1/sessions/{id}/context
- [ ] Implement PUT /api/v1/sessions/{id}/context
- [ ] Implement GET /api/v1/sessions/{id}/models
- [ ] Message storage and retrieval
- [ ] Context window tracking and management
- [ ] File attachment handling
- [ ] Tool call recording

### Verification

- [ ] Chat messages persist and retrieve correctly
- [ ] Context window usage calculated accurately
- [ ] Adding/removing context files updates token counts
- [ ] Tool calls captured in message history
- [ ] Available models listed with capabilities
- [ ] Message pagination works correctly
- [ ] Attachments stored and retrieved
- [ ] Context updates validated (files exist, tools available)

### Test Strategy

- Chat interaction tests simulating multi-turn conversations
- Context management tests adding/removing files
- Token counting tests validating calculations
- Attachment handling tests with various file types
- Model listing tests ensuring correct capabilities reported

---

## Milestone 11: Production Server - Timeline and Time-Travel

**Status**: Not Started

### Deliverables

- [ ] Implement GET /api/v1/sessions/{id}/timeline
- [ ] Implement POST /api/v1/sessions/{id}/fs-snapshots
- [ ] Implement POST /api/v1/sessions/{id}/moments
- [ ] Implement POST /api/v1/sessions/{id}/seek
- [ ] Implement POST /api/v1/sessions/{id}/session-branch
- [ ] Implement GET /api/v1/sessions/{id}/fs-snapshots
- [ ] Filesystem snapshot creation at tool boundaries
- [ ] Recording integration for timeline generation
- [ ] Snapshot mounting for inspection
- [ ] Session branching from snapshots

### Verification

- [ ] Timeline includes moments and fs_snapshots
- [ ] Manual snapshots created on demand
- [ ] Automatic snapshots created at tool boundaries
- [ ] Seek operation mounts snapshot read-only
- [ ] Session branching creates new session from snapshot
- [ ] Branched sessions start from snapshot state
- [ ] Recording data includes timing and events
- [ ] Snapshot metadata includes size and provider details

### Test Strategy

- Timeline generation tests with real task execution
- Snapshot creation tests with various FS providers
- Seek operation tests mounting snapshots
- Branching tests creating divergent sessions
- Recording playback tests validating timeline accuracy

---

## Milestone 12: Production Server - Draft and Repository Management

**Status**: Not Started

### Deliverables

- [ ] Implement POST /api/v1/drafts
- [ ] Implement GET /api/v1/drafts
- [ ] Implement PUT /api/v1/drafts/{id}
- [ ] Implement DELETE /api/v1/drafts/{id}
- [ ] Implement GET /api/v1/repos
- [ ] Implement GET /api/v1/projects
- [ ] Implement GET /api/v1/workspaces
- [ ] Implement GET /api/v1/workspaces/{id}
- [ ] Draft task persistence and retrieval
- [ ] Repository catalog management

### Verification

- [ ] Drafts created and persisted correctly
- [ ] Drafts listed with complete metadata
- [ ] Draft updates save incrementally
- [ ] Draft deletion removes data completely
- [ ] Repository listing returns indexed repositories
- [ ] Project listing returns tenant-scoped projects
- [ ] Workspace listing shows active workspaces
- [ ] Workspace detail view includes task history

### Test Strategy

- CRUD tests for draft lifecycle
- Repository catalog tests with various VCS types
- Workspace management tests
- Data persistence tests ensuring durability
- Concurrent draft editing tests

---

## Milestone 13: Production Server - Authentication and Authorization

**Status**: Not Started

### Deliverables

- [ ] API Key authentication support
- [ ] JWT bearer token authentication
- [ ] OIDC integration (Auth0/Keycloak)
- [ ] RBAC implementation (admin, operator, viewer roles)
- [ ] Tenant and project scoping
- [ ] Rate limiting per tenant/user
- [ ] Audit logging for privileged operations

### Verification

- [ ] API Key authentication accepts valid keys
- [ ] Invalid API keys rejected with 401
- [ ] JWT tokens validated correctly
- [ ] OIDC login flow works end-to-end
- [ ] RBAC enforces role permissions
- [ ] Admins can create tasks, viewers cannot
- [ ] Tenant isolation prevents cross-tenant access
- [ ] Rate limits enforced per tenant
- [ ] Audit log captures all privileged operations

### Test Strategy

- Authentication tests with valid/invalid credentials
- Authorization tests for each role
- Tenant isolation tests ensuring data separation
- Rate limiting tests per tenant/user
- OIDC integration tests with test identity provider
- Audit log tests validating captured events

---

## Milestone 14: CLI Integration - `ah webui`

**Status**: Not Started

### Deliverables

- [ ] Implement `ah webui` command in ah-cli
- [ ] In-process access point daemon startup
- [ ] WebUI server process management
- [ ] Port allocation and conflict detection
- [ ] Configuration passing to embedded server
- [ ] Graceful shutdown on Ctrl-C
- [ ] Browser auto-launch (optional)

### Verification

- [ ] `ah webui` starts access point daemon successfully
- [ ] WebUI accessible at <http://127.0.0.1:PORT>
- [ ] Server uses configured bind address and port
- [ ] Port conflict detected and reported clearly
- [ ] Ctrl-C shuts down both daemon and WebUI cleanly
- [ ] Browser opens to WebUI URL when configured
- [ ] Server logs show initialization and shutdown events
- [ ] Configuration flags (--port, --bind, --max-concurrent-tasks) work correctly

### Test Strategy

- CLI integration tests spawning `ah webui`
- Port conflict tests ensuring clear error messages
- Shutdown tests validating graceful cleanup
- Configuration tests with various flag combinations
- Browser launch tests (optional, platform-specific)

---

## Milestone 15: CLI Integration - `ah agent access-point`

**Status**: Not Started

### Deliverables

- [ ] Implement `ah agent access-point` command in ah-cli
- [ ] Standalone server mode (no WebUI)
- [ ] QUIC control plane for executor enrollment
- [ ] HTTP CONNECT handler for SSH tunneling
- [ ] Database configuration and initialization
- [ ] Dual-role support (server + executor with --max-concurrent-tasks > 0)
- [ ] Daemon process management

### Verification

- [ ] `ah agent access-point` starts server successfully
- [ ] Server listens on configured address/port
- [ ] Health check endpoint responds correctly
- [ ] QUIC control plane accepts executor connections
- [ ] HTTP CONNECT tunnels SSH to executors
- [ ] Server executes tasks when --max-concurrent-tasks > 0
- [ ] Database initialized with correct schema
- [ ] Server runs as daemon with proper process management

### Test Strategy

- CLI integration tests starting access-point
- Executor enrollment tests via QUIC
- SSH tunneling tests using HTTP CONNECT
- Task execution tests in dual-role mode
- Database initialization and migration tests
- Daemon lifecycle tests (start, stop, restart)

---

## Milestone 16: CLI Integration - `ah agent enroll`

**Status**: Not Started

### Deliverables

- [ ] Implement `ah agent enroll` command in ah-cli
- [ ] Executor enrollment via QUIC control plane
- [ ] SPIFFE identity provider integration
- [ ] Certificate-based authentication (files, Vault, exec)
- [ ] Heartbeat mechanism
- [ ] Resource capability reporting
- [ ] SSH tunnel enablement
- [ ] Optional REST API serving in enrolled executor

### Verification

- [ ] Executor enrolls with access point successfully
- [ ] SPIFFE X.509 SVID obtained and used for mTLS
- [ ] File-based certificates loaded and validated
- [ ] Heartbeats sent at configured intervals
- [ ] Resource capabilities reported accurately
- [ ] SSH tunnel accepts CONNECT requests
- [ ] Executor receives and executes assigned tasks
- [ ] REST API serves when --rest-api yes

### Test Strategy

- Enrollment tests with various identity providers
- Certificate validation tests
- Heartbeat tests ensuring connection kept alive
- Task assignment tests from access point to executor
- SSH tunnel tests through HTTP CONNECT
- REST API tests when enabled on executor

---

## Milestone 17: WebUI Proxy Integration

**Status**: Not Started

### Deliverables

- [ ] WebUI SSR server proxies `/api/v1/*` to access point daemon
- [ ] Request forwarding with header preservation
- [ ] SSE stream proxying with proper connection handling
- [ ] User access policies and security controls
- [ ] In-process vs subprocess daemon modes

### Verification

- [ ] WebUI SSR server forwards API requests correctly
- [ ] Authentication headers preserved through proxy
- [ ] SSE streams work through proxy
- [ ] User access policies enforced by SSR server
- [ ] In-process daemon mode works correctly
- [ ] Subprocess daemon mode works correctly
- [ ] Proxy handles connection errors gracefully

### Test Strategy

- Proxy integration tests with WebUI and access point
- SSE proxying tests ensuring event streams work
- Access policy tests with various user roles
- Daemon mode tests (in-process and subprocess)
- Error handling tests (daemon down, connection loss)

---

## Milestone 18: End-to-End Integration Testing

**Status**: Not Started

### Deliverables

- [ ] Full stack integration tests (CLI → Server → Task Execution)
- [ ] Multi-session concurrent execution tests
- [ ] Fleet orchestration tests (if implemented)
- [ ] Failure recovery tests (server crash, network issues)
- [ ] Performance benchmarks and optimization
- [ ] Load testing infrastructure
- [ ] Observability integration tests (metrics, tracing)

### Verification

- [ ] Tasks created via CLI execute successfully on server
- [ ] Multiple concurrent sessions execute without interference
- [ ] Session events stream correctly to multiple clients
- [ ] Server recovers gracefully from crashes
- [ ] Network interruptions handled with reconnection
- [ ] Performance meets target thresholds (TBD)
- [ ] Metrics exported correctly (Prometheus/OpenTelemetry)
- [ ] Distributed traces connect client → server → agent

### Test Strategy

- End-to-end scenario tests covering complete workflows
- Chaos testing (kill processes, disconnect network, fill disk)
- Load testing with progressively higher concurrency
- Observability validation tests
- Client compatibility tests (REST client, TUI, WebUI)

---

## Implementation Notes

### Development Order

1. **Milestone 1** (API Contract) and **Milestone 2** (TaskManager Trait) - foundation for all other work
2. **Milestone 3** (Mock Client) for TUI testing with simulated tokio time
3. **Milestone 4** (Production Client) tested against existing webui/mock-server
4. Both clients implement TaskManager trait, enabling drop-in replacement
5. Production server milestones (5-13) can proceed incrementally
6. CLI integration (14-16) once server is functional
7. WebUI proxy (17) after CLI integration complete
8. End-to-end testing (18) throughout, but comprehensive suite at end

**Key Dependencies:**

- Milestones 3 and 4 both depend on Milestones 1 and 2
- Milestone 3 (mock client) enables TUI development to continue independently
- Milestone 4 (production client) tests against existing webui/mock-server (may identify needed improvements)

### Testing Philosophy

- **Contract tests** ensure client and server agree on API shape
- **Mock client** enables TUI testing with simulated time (tokio::time::pause)
  - Supports MVVM-style testing as demonstrated in ah-tui crate
  - ViewModels tested independently with accelerated time simulation
  - See MVVM testing patterns in the ah-tui crate
- **Existing mock server** (webui/mock-server) validates production client behavior
- **Integration tests** validate real behavior against specifications
- **Property-based tests** for serialization and validation logic
- **Chaos tests** ensure resilience and recovery
- **Load tests** identify performance bottlenecks early

### Key Technical Decisions

- **SQLite** for initial state backend (can migrate to Postgres later)
- **Axum** for HTTP framework (mature, well-integrated with Tower ecosystem)
- **utoipa** for OpenAPI generation (compile-time checking)
- **SSE over WebSocket** (simpler, better HTTP compatibility, easier proxy)
- **In-process daemon** for `ah webui` (simpler deployment, lower latency)
- **QUIC control plane** for executor enrollment (better NAT traversal, multiplexing)

### Future Enhancements (Post-MVP)

- Postgres backend for scaled deployments
- Redis for distributed state and pub/sub
- WebSocket support alongside SSE
- gRPC control plane as alternative to QUIC
- Kubernetes operator for cluster deployments
- Multi-tenancy with full isolation
- Advanced RBAC with fine-grained permissions
- Secrets management integration (Vault, AWS Secrets Manager)

---

## Dependencies and Prerequisites

### Before Starting Implementation

- [ ] Review and finalize [API.md](API.md) specification
- [ ] Review and finalize [Tech-Stack.md](Tech-Stack.md) decisions
- [ ] Confirm [Repository-Layout.md](../Repository-Layout.md) crate structure
- [ ] Ensure [CLI.md](../CLI.md) accurately describes `ah` commands
- [ ] Verify [Local-Mode.md](../Local-Mode.md) and [Remote-Mode.md](../Remote-Mode.md) are aligned

### External Dependencies

- Rust toolchain (specified in rust-toolchain.toml)
- SQLite development libraries
- SPIFFE Workload API (for executor enrollment)
- OpenSSL or rustls (for TLS)
- Git (for repository operations)

### Development Tools

- `cargo` for building and testing
- `sqlx-cli` for database migrations
- `wiremock` for HTTP mocking in tests
- `openapi-validator` for schema validation
- Load testing tools (wrk, k6, or custom)

---

## Success Criteria

The REST service implementation is considered complete when:

1. All milestones have passing verification tests
2. Full [API.md](API.md) specification implemented in production server
3. TaskManager trait defined in ah-core enables abstraction over local/remote execution
4. Mock client enables TUI testing with simulated tokio time
5. Production REST client tested against existing webui/mock-server
6. Both clients are drop-in replacements via TaskManager trait
7. `ah webui` and `ah agent access-point` commands work as specified in [CLI.md](../CLI.md)
8. Tasks launched remotely execute via `ah agent start` wrapped in `ah agent record`
9. End-to-end workflows (create task → execute → stream events → view results) work reliably
10. Documentation complete with examples and troubleshooting guides
11. Performance meets target thresholds (TBD based on load testing)
12. Security model (authentication, authorization, audit logging) fully functional
