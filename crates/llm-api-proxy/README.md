# LLM API Proxy

A Rust library providing LLM API proxying, routing, and scenario playback capabilities.

## Features

### Core Functionality

The library provides essential proxying capabilities for routing LLM API requests between clients and providers.

#### HTTP Client with Provider Routing

Routes requests through the implemented OpenRouter integration and applies provider-specific configuration such as weighted routing and authentication. Additional providers can be layered in via `ProxyConfig` without altering the library code.

#### Dynamic Session-Based Routing

Provides runtime configuration of routing rules on a per-session basis through REST API endpoints. Sessions are identified by API keys and can have custom routing configurations applied dynamically, with automatic cleanup after inactivity periods.

- **Session Preparation**: POST `/prepare-session` endpoint accepts API key, providers list, model mappings, and default provider
- **Flexible Provider Configuration**: Each provider specifies base URL and custom headers (typically for authentication)
- **Model Pattern Matching**: Case-insensitive substring matching for routing models to providers
- **Automatic Application**: Requests with configured API keys automatically use their assigned routing rules
- **Configuration Caching**: Identical configurations are deduplicated with reference counting
- **Session Lifecycle**: Automatic expiration after 3 days of inactivity or explicit cleanup via `/end-session`

#### Configuration System

YAML-based configuration for defining providers, their API endpoints, authentication credentials, and routing rules. Supports environment variable substitution for sensitive data like API keys.

#### Basic Metrics Collection

Tracks request latency, success/failure counts, and token usage. Thread-safe counters ensure accurate metrics even under concurrent load. Useful for monitoring proxy performance and provider reliability.

#### Asynchronous Request Processing

Built on Tokio for high-performance async processing. Handles multiple concurrent requests efficiently without blocking, making it suitable for production workloads.

#### API Format Detection and Bidirectional Conversion

Automatically detects whether incoming requests use OpenAI or Anthropic API formats, performs request/response translation (including tool calls, usage accounting, and streaming deltas), and forwards them to the appropriate provider.

### Scenario Playback

Provides deterministic testing capabilities by replaying recorded interaction scenarios.

#### Deterministic Scenario Execution

Executes pre-recorded scenarios with predictable outcomes, enabling reliable integration testing. Scenarios can include LLM responses, user inputs, assertions, and file system operations.

#### Timeline-based Event Processing

Processes scenario events in chronological order with proper timing, simulating real user interactions and system responses for comprehensive testing.

#### HTTP Test Server

Built-in test server that serves scenario responses over HTTP, enabling end-to-end integration testing without external API calls.

### Test Server

Command-line interface specifically designed for testing and development workflows.

#### Command-line Interface for Integration Testing

Standalone executable for testing proxy functionality. Supports various configuration options and provides detailed output for debugging integration issues.

#### Configurable Request/Response Logging

Flexible logging system that can capture requests, responses, or both. Supports JSON output for easy parsing and includes options to log headers, bodies, or both selectively.

#### Scenario File Support

Loads scenario definitions from YAML files, allowing you to define complex interaction sequences for testing specific workflows or edge cases.

#### Multiple Provider Compatibility

Supports weighted round-robin selection across multiple provider replicas. When fallback routing is enabled, the router will select among providers that share a logical name (e.g., regional OpenRouter deployments).

### Current Limitations

- Streaming conversions currently cover text deltas and tool call payloads; additional content types (audio, images) are skipped with warnings.
- Only the OpenRouter HTTP path is validated end-to-end today. Adding direct Anthropic/OpenAI backends may require tweaking provider settings and credentials.
- Scenario playback follows the legacy Python mock server timeline logic but does not yet manage filesystem state or advanced harness assertions described in `Scenario-Format.md`.
- The command-line test server is still stabilizing; expect breaking changes while logging flags and Clap-based options evolve.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
llm-api-proxy = { path = "../llm-api-proxy" }
tokio = { version = "1.0", features = ["full"] }
```

## Usage

### Library Usage

#### Basic Proxy Setup

First, add the dependency to your `Cargo.toml`:

```toml
[dependencies]
llm-api-proxy = { path = "../llm-api-proxy" }
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
```

#### Creating a Proxy Instance

```rust
use llm_api_proxy::{LlmApiProxy, ProxyConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from file or use defaults
    let config = ProxyConfig::default();

    // Create the proxy instance
    // This initializes the routing system and prepares providers
    let proxy = LlmApiProxy::new(config).await?;

    Ok(())
}
```

#### Dynamic Session Configuration

Configure routing rules for specific API keys through REST endpoints:

```rust
use serde_json::json;

// Prepare a session with custom routing
let prepare_request = json!({
    "api_key": "sk-session-123",
    "providers": [
        {
            "name": "anthropic",
            "base_url": "https://api.anthropic.com",
            "headers": {
                "anthropic-version": "2023-06-01",
                "authorization": "Bearer sk-ant-api-key-here"
            }
        },
        {
            "name": "openai",
            "base_url": "https://api.openai.com/v1",
            "headers": {
                "authorization": "Bearer sk-openai-key"
            }
        }
    ],
    "model_mappings": [
        {
            "source_pattern": "haiku",
            "provider": "anthropic",
            "model": "claude-3-5-haiku-20241022"
        },
        {
            "source_pattern": "opus",
            "provider": "anthropic",
            "model": "claude-3-opus-20240229"
        },
        {
            "source_pattern": "gpt-4",
            "provider": "openai",
            "model": "gpt-4o"
        }
    ],
    "default_provider": "anthropic"
});

// All subsequent requests with this API key will use the configured routing
// curl -X POST http://localhost:18081/prepare-session \
//   -H "Content-Type: application/json" \
//   -d '{"api_key": "sk-session-123", "providers": [...], "model_mappings": [...], "default_provider": "anthropic"}'

// End a session explicitly
// curl -X POST http://localhost:18081/end-session \
//   -H "Content-Type: application/json" \
//   -d '{"api_key": "sk-session-123"}'
```

#### Proxying Requests

The core functionality is routing LLM API requests to appropriate providers:

```rust
use llm_api_proxy::{ProxyRequest, proxy::ProxyMode, converters::ApiFormat};

// Create a request to proxy
let request = ProxyRequest {
    // Specify which API format the client is using
    client_format: ApiFormat::Anthropic,  // or ApiFormat::OpenAI

    // The actual API request payload
    payload: serde_json::json!({
        "model": "claude-3-sonnet-20240229",
        "messages": [
            {"role": "user", "content": "Explain quantum computing in simple terms"}
        ],
        "max_tokens": 1000
    }),

    // HTTP headers (authorization, content-type, etc.)
    headers: HashMap::from([
        ("authorization".to_string(), "Bearer your-api-key".to_string()),
        ("content-type".to_string(), "application/json".to_string()),
    ]),

    // Unique identifier for this request
    request_id: "req-123".to_string(),

    // Live mode routes to real providers, Scenario mode uses test data
    mode: ProxyMode::Live,
};

// Send the request through the proxy
// The proxy will automatically route based on the format and configuration
let response = proxy.proxy_request(request).await?;

println!("Response received: {:?}", response.payload);
```

#### Configuration Example

```rust
use llm_api_proxy::config::{ProxyConfig, ProviderConfig};

// Create a custom configuration
let mut config = ProxyConfig::default();

// Configure a provider (e.g., OpenRouter)
let openrouter = ProviderConfig {
    name: "openrouter".to_string(),
    base_url: "https://openrouter.ai/api/v1".to_string(),
    api_key: Some("sk-or-v1-...".to_string()),
    headers: HashMap::new(),
};

// Set routing preferences
config.routing.default_provider = Some("openrouter".to_string());

// Create proxy with custom config
let proxy = LlmApiProxy::new(config).await?;
```

### Test Server Usage

The test server provides a standalone executable for testing and development workflows.

#### Starting a Basic Test Server

```bash
# Run with default settings (minimal output)
cargo run -p llm-api-proxy -- test-server

# Specify a scenario file for mock responses
cargo run -p llm-api-proxy -- test-server \
  --scenario-file path/to/scenario.yaml \
  --agent-type claude
```

#### Test Server with Comprehensive Logging

```bash
# Enable full request/response logging for debugging
cargo run -p llm-api-proxy -- test-server \
  --scenario-file scenarios/realistic_development_scenario.yaml \
  --agent-type claude \
  --request-log test-session.log \
  --log-headers \
  --log-body \
  --log-responses
```

This command:

- Starts a test server on port 18081
- Loads scenario responses from the YAML file
- Logs all HTTP traffic to `test-session.log`
- Includes request headers, request bodies, and response payloads in logs

#### Session-Based Routing

Configure per-session routing rules dynamically:

```bash
# Prepare a session with custom Anthropic routing
curl -X POST http://localhost:18081/prepare-session \
  -H "Content-Type: application/json" \
  -d '{
    "api_key": "sk-session-custom-123",
    "providers": [
      {
        "name": "anthropic",
        "base_url": "https://api.anthropic.com",
        "headers": {
          "anthropic-version": "2023-06-01",
          "authorization": "Bearer sk-ant-your-anthropic-key"
        }
      }
    ],
    "model_mappings": [
      {
        "source_pattern": "claude",
        "provider": "anthropic",
        "model": "claude-3-sonnet-20240229"
      }
    ],
    "default_provider": "anthropic"
  }'

# Now requests with this API key will use Anthropic
curl -X POST http://localhost:18081/v1/messages \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer sk-session-custom-123" \
  -d '{
    "model": "claude-3-sonnet-20240229",
    "messages": [{"role": "user", "content": "Hello from custom session!"}],
    "max_tokens": 100
  }'

# End the session explicitly
curl -X POST http://localhost:18081/end-session \
  -H "Content-Type: application/json" \
  -d '{"api_key": "sk-session-custom-123"}'
```

#### Testing with curl

Once the test server is running:

```bash
# Test Anthropic API format
curl -X POST http://localhost:18081/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-sonnet-20240229",
    "messages": [{"role": "user", "content": "Hello, test server!"}],
    "max_tokens": 100
  }'

# Test OpenAI API format
curl -X POST http://localhost:18081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello, test server!"}],
    "max_tokens": 100
  }'
```

#### Understanding Log Output

The test server produces detailed JSON logs for debugging:

**Request Log Entry:**

```json
{
  "timestamp": "2025-10-21T10:45:30.123Z",
  "type": "request",
  "method": "POST",
  "path": "/v1/messages",
  "request_id": "req-uuid-123",
  "client_format": "Anthropic",
  "scenario": "test_scenario",
  "headers": {
    "authorization": "Bearer sk-ant-...",
    "content-type": "application/json"
  },
  "body": {
    "model": "claude-3-sonnet",
    "messages": [{ "role": "user", "content": "Hello" }]
  }
}
```

**Response Log Entry:**

```json
{
  "timestamp": "2025-10-21T10:45:30.456Z",
  "type": "response",
  "method": "POST",
  "path": "/v1/messages",
  "request_id": "req-uuid-123",
  "scenario": "test_scenario",
  "response": {
    "content": [{ "text": "Hello! How can I help you?" }],
    "usage": { "input_tokens": 10, "output_tokens": 8 }
  }
}
```

## Configuration

### Provider Configuration

The proxy uses YAML configuration files to define providers and routing rules:

```yaml
# config.yaml
server:
  # Port for the proxy server (when used as a service)
  port: 8080

providers:
  # Define available LLM providers
  openrouter:
    # API key for authentication (can use env vars)
    api_key: '${OPENROUTER_API_KEY}'
    # Base URL for the provider's API
    base_url: 'https://openrouter.ai/api/v1'
    # Optional custom headers
    headers:
      'X-Custom-Header': 'value'

  anthropic:
    api_key: '${ANTHROPIC_API_KEY}'
    base_url: 'https://api.anthropic.com'
    # Additional headers for this provider
    headers:
      'anthropic-version': '2023-06-01'

routing:
  # Default provider for requests that don't match specific rules
  default_provider: 'openrouter'

  # Optional: model-based routing rules
  model_mappings:
    'claude-3-sonnet': 'anthropic'
    'gpt-4': 'openai'
```

### Environment Variables

Sensitive configuration like API keys should use environment variables:

```bash
export OPENROUTER_API_KEY="sk-or-v1-your-key-here"
export ANTHROPIC_API_KEY="sk-ant-your-key-here"
cargo run -p llm-api-proxy -- test-server --scenario-file scenario.yaml
```

### Test Server Command Line Options

The test server accepts many options for customization:

```bash
cargo run -p llm-api-proxy -- test-server --help
```

**Server Configuration:**

- `--port <PORT>`: Port to bind the server (default: 18081)
- `--scenario-file <FILE>`: Path to YAML scenario file for mock responses
- `--agent-type <TYPE>`: Agent type for scenario compatibility (default: codex)
- `--agent-version <VERSION>`: Agent version string for scenario matching (default: unknown)

**Tool Validation:**

- `--strict-tools-validation`: Enable strict validation of tool definitions in requests

**Logging Options:**

- `--request-log <PATH>`: File path to write request/response logs (enables logging)
- `--log-headers`: Include HTTP headers in log entries
- `--log-body`: Include request body content in logs
- `--log-responses`: Include response payloads in logs

**Logging Behavior:**

- Logging is disabled by default for privacy and performance
- Specify `--request-log <file>` to enable logging to a file
- Individual content types (headers, body, responses) can be enabled/disabled separately
- All logs are written as JSON Lines format for easy parsing

## API Reference

### Core Types

#### LlmApiProxy

The main proxy struct that handles request routing and provider management.

```rust
pub struct LlmApiProxy {
    config: ProxyConfig,
    router: DynamicRouter,
    metrics: MetricsCollector,
    scenario_player: Option<ScenarioPlayer>,
}
```

#### ProxyRequest

Represents an incoming API request to be proxied.

```rust
pub struct ProxyRequest {
    /// The API format of the incoming request (OpenAI or Anthropic)
    pub client_format: ApiFormat,

    /// The JSON payload of the API request
    pub payload: serde_json::Value,

    /// HTTP headers from the client request
    pub headers: HashMap<String, String>,

    /// Unique identifier for request tracking
    pub request_id: String,

    /// Whether to use live providers or scenario playback
    pub mode: ProxyMode,
}
```

#### ProxyResponse

Represents the response from a proxied request.

```rust
pub struct ProxyResponse {
    /// The JSON response payload from the provider
    pub payload: serde_json::Value,

    /// Unique identifier for response tracking
    pub response_id: String,

    /// HTTP status code from the provider
    pub status: u16,

    /// Response headers from the provider
    pub headers: HashMap<String, String>,
}
```

#### Supporting Enums

```rust
/// Supported API formats for detection and routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiFormat {
    OpenAI,        // OpenAI API format
    OpenAIResponses, // OpenAI Responses API format
    Anthropic,     // Anthropic API format
}

/// Request processing modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    Live,      // Route to real LLM providers
    Scenario,  // Use scenario playback for testing
}
```

### REST API Endpoints

#### Session Management Endpoints

##### POST `/prepare-session`

Prepares a session with custom routing configuration for a specific API key.

**Request Body:**

```json
{
  "api_key": "sk-session-123",
  "providers": [
    {
      "name": "anthropic",
      "base_url": "https://api.anthropic.com",
      "headers": {
        "anthropic-version": "2023-06-01",
        "authorization": "Bearer sk-ant-your-key"
      }
    },
    {
      "name": "openai",
      "base_url": "https://api.openai.com/v1",
      "headers": {
        "authorization": "Bearer sk-openai-key"
      }
    }
  ],
  "model_mappings": [
    {
      "source_pattern": "haiku",
      "provider": "anthropic",
      "model": "claude-3-5-haiku-20241022"
    },
    {
      "source_pattern": "opus",
      "provider": "anthropic",
      "model": "claude-3-opus-20240229"
    },
    {
      "source_pattern": "gpt-4",
      "provider": "openai",
      "model": "gpt-4o"
    }
  ],
  "default_provider": "anthropic"
}
```

**Response:**

```json
{
  "status": "success",
  "session_id": "session-uuid-123",
  "expires_at": "2025-10-24T10:30:00Z"
}
```

##### POST `/end-session`

Explicitly ends a session and cleans up its routing configuration.

**Request Body:**

```json
{
  "api_key": "sk-session-123"
}
```

**Response:**

```json
{
  "status": "success",
  "message": "Session ended"
}
```

### Key Methods

#### LlmApiProxy Implementation

```rust
impl LlmApiProxy {
    /// Create a new proxy instance with the given configuration
    ///
    /// This initializes the routing system, loads provider configurations,
    /// and prepares metrics collection.
    pub async fn new(config: ProxyConfig) -> Result<Self>;

    /// Proxy an API request to the appropriate provider
    ///
    /// The request will be automatically routed based on its format,
    /// model specifications, and routing configuration.
    pub async fn proxy_request(&self, request: ProxyRequest) -> Result<ProxyResponse>;

    /// Access the metrics collector for monitoring
    pub fn metrics(&self) -> &MetricsCollector;
}
```

#### MetricsCollector

```rust
pub struct MetricsCollector {
    // Thread-safe counters for various metrics
}

impl MetricsCollector {
    /// Get total number of requests processed
    pub fn total_requests(&self) -> u64;

    /// Get average request latency in milliseconds
    pub fn average_latency_ms(&self) -> f64;

    /// Get total tokens processed
    pub fn total_tokens(&self) -> u64;
}
```

### Error Types

The library uses comprehensive error handling:

```rust
use llm_api_proxy::Error;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Configuration error: {message}")]
    Config { message: String },

    #[error("Provider error: {provider}, status: {status}, message: {message}")]
    Provider { provider: String, status: u16, message: String },

    #[error("Scenario error: {message}")]
    Scenario { message: String },

    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
```

## Logging

The test server provides comprehensive logging capabilities for debugging and monitoring API interactions.

### Log Format

All logs are written as JSON Lines (one JSON object per line) for easy parsing and analysis. This format is compatible with tools like `jq`, log aggregation systems, and can be easily imported into databases or analytics platforms.

### Logging Architecture

The logging system captures the complete request-response cycle:

1. **Request Logging**: Captures incoming API requests before they are processed
2. **Response Logging**: Captures outgoing API responses after they are received from providers
3. **Correlation**: Each request-response pair is linked by `request_id` for tracing

### Logging Options

#### Enabling Logging

```bash
# Enable all logging components
cargo run -p llm-api-proxy -- test-server \
  --request-log session.log \
  --log-headers \
  --log-body \
  --log-responses

# Selective logging - only headers and responses
cargo run -p llm-api-proxy -- test-server \
  --request-log session.log \
  --log-headers \
  --log-responses

# Minimal logging - just basic request info
cargo run -p llm-api-proxy -- test-server \
  --request-log session.log
```

#### Privacy and Performance Controls

```bash
# Disable sensitive content logging
cargo run -p llm-api-proxy -- test-server \
  --request-log session.log \
  --no-log-headers \
  --no-log-body \
  --log-responses

# Disable all logging (default)
cargo run -p llm-api-proxy -- test-server \
  --no-logging
```

### Log Content Details

#### Request Log Fields

| Field           | Type     | Description                     |
| --------------- | -------- | ------------------------------- |
| `timestamp`     | ISO 8601 | When the request was received   |
| `type`          | string   | Always `"request"`              |
| `method`        | string   | HTTP method (always `"POST"`)   |
| `path`          | string   | API endpoint path               |
| `request_id`    | string   | Unique request identifier       |
| `client_format` | string   | `"OpenAI"` or `"Anthropic"`     |
| `scenario`      | string   | Scenario name (for test server) |
| `api_key`       | string   | Masked API key identifier       |
| `headers`       | object   | HTTP headers (if enabled)       |
| `body`          | object   | Request payload (if enabled)    |

#### Response Log Fields

| Field        | Type     | Description                     |
| ------------ | -------- | ------------------------------- |
| `timestamp`  | ISO 8601 | When the response was sent      |
| `type`       | string   | Always `"response"`             |
| `method`     | string   | HTTP method (always `"POST"`)   |
| `path`       | string   | API endpoint path               |
| `request_id` | string   | Matching request identifier     |
| `scenario`   | string   | Scenario name (for test server) |
| `response`   | object   | Provider response payload       |

### Log Analysis Examples

```bash
# View all requests for a specific API key
jq 'select(.type == "request" and (.api_key | contains("sk-ant"))) | .timestamp, .path' session.log

# Count requests by API format
jq -r '.client_format // empty' session.log | sort | uniq -c

# Find slow requests (responses taking >1 second)
jq 'select(.type == "response") |
    select((.timestamp | fromdate) - (input | .timestamp | fromdate) > 1) |
    .request_id' session.log

# Extract all Anthropic API calls
jq 'select(.client_format == "Anthropic" and .type == "request") | .body.model' session.log
```

### Log File Management

- **Rotation**: Logs are appended to the specified file; implement external rotation if needed
- **Size**: Monitor file size as logs can grow quickly with full content logging
- **Security**: Logs may contain sensitive data; store and transmit securely
- **Performance**: Logging impacts throughput; disable in production if not needed

## Testing

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests
cargo test -- --ignored
```

### Test Server Usage

```bash
# Start test server for integration testing
cargo run -p llm-api-proxy -- test-server \
  --scenario-file scenario.yaml \
  --agent-type claude \
  --request-log session.log
```

## Examples

### Basic Usage

```rust
use llm_api_proxy::{LlmApiProxy, ProxyConfig, ProxyRequest};

let config = ProxyConfig::default();
let proxy = LlmApiProxy::new(config).await?;

let request = ProxyRequest {
    client_format: llm_api_proxy::converters::ApiFormat::Anthropic,
    payload: serde_json::json!({"model": "claude-3-sonnet", "messages": [{"role": "user", "content": "Hello"}]}),
    headers: std::collections::HashMap::new(),
    request_id: "req-123".to_string(),
    mode: llm_api_proxy::proxy::ProxyMode::Live,
};

let response = proxy.proxy_request(request).await?;
println!("Response: {:?}", response.payload);
```

## Metrics

Basic metrics collection is available:

```rust
let metrics = proxy.metrics();
println!("Total requests: {}", metrics.total_requests());
```

## Error Handling

```rust
use llm_api_proxy::Error;

match proxy.proxy_request(request).await {
    Ok(response) => println!("Success: {:?}", response),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

## Development

```bash
# Run tests
cargo test

# Run test server
cargo run -p llm-api-proxy -- test-server --scenario-file scenario.yaml
```
