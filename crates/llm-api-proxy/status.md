# LLM API Proxy — Status and Plan

Spec: See [Scenario-Format.md](../../specs/Public/Scenario-Format.md) for the scenario playback requirements. This file tracks the implementation plan, success criteria, and an automated test strategy per specs/AGENTS.md.

## Goal

Deliver a high-performance LLM API proxy library that can be integrated into `ah webui` to provide API translation, routing, metrics collection, and scenario playback capabilities. The library should translate between OpenAI and Anthropic API formats, route requests to multiple providers (OpenRouter, Anthropic, OpenAI, etc.), collect comprehensive metrics, and support deterministic playback of Scenario-Format scenarios.

## Current Status: ✅ Bidirectional proxy with scenario playback and Anthropic API compliance

**Implemented Features:**
- Bidirectional OpenAI ↔ Anthropic conversion for chat completions, responses API, tool calls, metadata, and streaming deltas
- Weighted round-robin routing with model-pattern rules, replica weights, and provider fallback selection
- HTTP client and configuration system with provider management, API key injection, and environment overrides
- Metrics collection (latency, request counts, token usage) integrated with proxy lifecycle
- Scenario playback engine aligned with `Scenario-Format.md`, including tool profile validation and request/response logging hooks
- Clap-based test server wiring converters, routing, and scenario playback for manual and automated runs
- Unit tests covering converters (bidirectional + streaming), routing selector, scenario playback, and configuration validation

**Current Limitations / Known Issues:**
- Multimodal payloads (audio/images) are skipped during conversion with warnings
- Metrics remain in-memory only (no telemetry export yet)
- CLI logging refactor is mid-flight; compile errors remain until Option handling / PathBuf formatting is fixed (tracked below)
- Other assertion types (text, JSON, git) are not yet implemented - only filesystem assertions (fs.exists, fs.notExists) are supported
- Citations field not yet implemented for Anthropic text content blocks

## Implementation Status

### ✅ Completed Features

1. **Core Infrastructure**
   - Async proxy architecture with request/response handling
   - Configuration management with YAML support and env overrides
   - Provider abstraction layer with API key management
   - HTTP client integration with authentication

2. **Routing & Provider Management**
   - HTTP API integration with OpenRouter
   - Model-pattern routing with weighted round-robin across provider replicas
   - Configurable fallback selection when logical provider names share replicas
   - Provider info hydration (base URL, headers, API keys)

3. **Bidirectional API Conversion**
   - OpenAI → Anthropic translations (system/developer/user messages, tool calls, metadata, streaming deltas)
   - Anthropic → OpenAI translations (tool_calls, finish reasons, usage accounting)
   - Stream chunk translation in both directions with delta reconciliation and warning surfacing

4. **Metrics & Observability**
   - Request/response latency tracking
   - Request count statistics with success/failure tallies
   - Token usage extraction from provider responses
   - Thread-safe counters shared across async tasks

5. **Test Server & Logging**
   - Clap-based `llm-api-proxy test-server` command with scenario selection
   - Axum routes for OpenAI `/chat/completions`, `/responses`, and Anthropic `/messages`
   - Request/response logging toggles controlled via env/CLI switches
   - ✅ Configurable JSON log formatting (pretty-print by default, minimize with `--minimize-logs`)

6. **Scenario Playback**
   - Timeline event processing following Scenario-Format semantics
   - Tool validation profiles with FORCE_TOOLS_VALIDATION_FAILURE capture path
   - Request/response log emission for regression tracking
   - ✅ Filesystem assertion events (`fs.exists`, `fs.notExists`) executed before next response
   - ✅ Initial prompt detection - waits for meaningful requests before starting scenario playback
   - ✅ Anthropic API response format compliance with required fields (`stop_sequence`, proper usage structure)

### 🔄 Outstanding Tasks

1. **CLI + Logging Refinement**
   - Resolve Clap/Option compile errors (PathBuf display, Option moves)
   - Ensure `manual-test-agent-start.py` forwards default logging flags to the server binary
   - Add regression tests for logging env propagation

2. **Enhanced Provider Support**
   - Integrate direct Anthropic and OpenAI provider clients alongside OpenRouter
   - Surface provider health metrics and fallback decisions
   - Replace stubbed Helicone router with real implementation when crates land

3. **Advanced Metrics & Telemetry**
   - Expand metrics to include per-provider error rates and streaming counters
   - Add OpenTelemetry export / scrape endpoints
   - Scenario playback metrics (timeline latency, validation outcomes)

4. **Complete Scenario Playback**
   - ✅ Implement remaining Scenario-Format events (filesystem assertions implemented, workspace mirroring pending)
   - Restore deterministic workspace orchestration from legacy Python mock
   - Provide HTTP control hooks for scenario load/reset/status

5. **WebUI Integration**
   - Document and harden the public API surface for `ah webui`
   - Offer concurrency-safe session management utilities
   - Add configuration loaders aligned with webui deployment environment

6. **Production Hardening**
   - Implement retry, backoff, and circuit breaker policies per provider
   - Add rate limiting and abuse protection layers
   - Integrate secure credential storage / rotation flows

7. **Testing & Benchmarks**
   - Golden tests covering conversion edge cases (multimodal, refusal blocks, tool deltas)
   - Integration tests for routing fallback and multi-provider failover
   - Performance/load benchmarks for high-concurrency streaming sessions

## Testing

### Current Test Coverage

- Unit tests for core functionality (converter/routing/scenario modules) — blocked until CLI compile fixes land
- HTTP client and routing logic tests
- Configuration validation tests
- Metrics collection tests
- Converter-specific streaming tests (`tests/converter_tests.rs`)

### Test Server Validation

The test server enables manual integration testing with:
- Real HTTP request/response cycles
- Configurable logging for debugging
- Scenario playback validation
- Provider compatibility testing

### Future Test Expansion

When additional features are implemented:
- API conversion accuracy tests (with golden files)
- Advanced scenario playback validation
- Performance benchmarks
- Multi-provider failover testing
- Concurrent load testing

## Implementation Details

### Architecture

```
llm-api-proxy/
├── src/
│   ├── lib.rs                 # Main library interface
│   ├── config.rs              # Configuration management
│   ├── proxy.rs               # Core proxy logic & routing
│   ├── converters/            # Bidirectional OpenAI ↔ Anthropic converters
│   ├── routing/               # Provider routing logic
│   ├── metrics/               # Basic telemetry
│   ├── scenario/              # Scenario playback engine
│   └── error.rs               # Error types and handling
├── tests/                     # Unit tests
└── README.md                  # User documentation
```

### Key Dependencies

- `axum` - HTTP server framework
- `reqwest` - HTTP client for provider requests
- `serde` - Serialization/deserialization
- `tokio` - Async runtime
- `clap` - Command-line argument parsing

### Library API

```rust
pub struct LlmApiProxy { /* ... */ }

impl LlmApiProxy {
    pub async fn new(config: ProxyConfig) -> Result<Self>;
    pub async fn proxy_request(&self, request: ProxyRequest) -> Result<ProxyResponse>;
}
```

## Success Criteria

### Current Phase Goals
- **Core Functionality**: HTTP proxy with provider routing and basic metrics
- **Test Server**: Functional CLI for integration testing with logging
- **Scenario Playback**: Basic timeline execution framework
- **Library API**: Usable interface for external integration

### Future Phase Goals
- **API Translation**: Bidirectional OpenAI↔Anthropic conversion with 100% field preservation
- **Advanced Routing**: Multi-provider support with load balancing and failover
- **Enhanced Metrics**: Comprehensive telemetry with OpenTelemetry export
- **Complete Scenario Playback**: Full Scenario-Format support with deterministic execution
- **Production Features**: Retry logic, circuit breakers, rate limiting
- **Security**: Enhanced authentication and credential management
- **Performance**: Sub-100ms latency for API translation, support for 1000+ concurrent requests
